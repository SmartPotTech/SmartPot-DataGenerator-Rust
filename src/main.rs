//! Punto de entrada del simulador SmartPot DataGenerator en Rust.
#![deny(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use smartpot_datagenerator::api::{create_router, AppState};
use smartpot_datagenerator::config::load;
use smartpot_datagenerator::domain::weather::WeatherClient;
use smartpot_datagenerator::pots::PotManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--healthcheck") {
        let port = std::env::var("SIMULATOR_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8081);

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => std::process::exit(1),
        };

        match client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => std::process::exit(0),
            _ => std::process::exit(1),
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,smartpot_datagenerator=debug")),
        )
        .init();

    info!(
        "Iniciando SmartPot DataGenerator v{}",
        smartpot_datagenerator::version()
    );

    let settings = match load() {
        Ok(s) => s,
        Err(err) => {
            error!("Error cargando configuración: {err}");
            std::process::exit(1);
        }
    };

    let weather_client = WeatherClient::default();
    let manager = Arc::new(PotManager::new(settings.clone(), weather_client));

    if let Err(err) = manager.load_static().await {
        error!("Error cargando dispositivos estáticos: {err}");
        std::process::exit(1);
    }

    manager.start().await;

    let api_status = if settings.api_token.is_some() {
        "activa"
    } else {
        "deshabilitada (falta SIMULATOR_TOKEN)"
    };

    info!(
        "Simulador iniciado con {} cultivos fijos; API de control {}",
        settings.devices.len(),
        api_status
    );

    let state = AppState {
        manager: Arc::clone(&manager),
        api_token: settings.api_token.clone(),
    };

    let app = create_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], settings.api_port));
    info!("API de control escuchando en http://{addr}");

    let listener = TcpListener::bind(addr).await?;

    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Fallo al registrar manejador de señal Ctrl+C");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    manager.stop().await;
    info!("Simulador detenido");

    Ok(())
}
