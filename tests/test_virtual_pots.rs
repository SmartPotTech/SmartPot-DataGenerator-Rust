//! Pruebas de integración del administrador de cultivos virtuales y la API HTTP.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

use smartpot_datagenerator::api::{create_router, AppState};
use smartpot_datagenerator::config::{DeviceConfig, Settings};
use smartpot_datagenerator::domain::actuator::{Action, Actuator};
use smartpot_datagenerator::domain::environment::Environment;
use smartpot_datagenerator::domain::weather::{condition, WeatherClient};
use smartpot_datagenerator::pots::{PotConfig, PotManager};

const TOKEN: &str = "test-simulator-token-0123456789";
const CROP: &str = "66f5a1000000000000000201";
const KEY: &str = "device-key-for-tests-0001";

fn test_settings() -> Settings {
    Settings {
        host: "localhost".to_string(),
        port: 1883,
        tls: false,
        ca_file: None,
        topic_prefix: "smartpot/v1".to_string(),
        interval_seconds: 30.0,
        devices: vec![DeviceConfig {
            crop_id: "66f5a1000000000000000101".to_string(),
            key: "demo-key".to_string(),
            crop_type: "LETTUCE".to_string(),
        }],
        utc_offset_hours: -5.0,
        api_token: Some(TOKEN.to_string()),
        api_port: 8081,
    }
}

#[test]
fn test_weather_codes_map_to_scenes() {
    assert_eq!(condition(0), ("CLEAR", "Despejado"));
    assert_eq!(condition(63).0, "RAIN");
    assert_eq!(condition(95).0, "STORM");
    assert_eq!(condition(1234), ("CLOUDY", "Nublado"));
}

#[tokio::test]
async fn test_manual_gauges_change_the_reading_and_actuators_still_act() {
    let mut env = Environment::new("LETTUCE", Some(1), 0.0);
    let _ = env.set_mode("MANUAL");

    let mut manual = HashMap::new();
    manual.insert("soilMoisture".to_string(), 35.0);
    manual.insert("temperature".to_string(), 30.0);
    env.set_manual(&manual);

    assert_eq!(env.state.get("soilMoisture").copied().unwrap(), 35.0);

    let _ = env.apply(Actuator::WaterPump, Action::Activate, Some(15), 100.0);
    env.step(60.0, 160.0);
    assert!(env.state.get("soilMoisture").copied().unwrap() > 45.0);

    for m in 1..60 {
        env.step(60.0, 160.0 + m as f64 * 60.0);
    }
    let temp = env.state.get("temperature").copied().unwrap();
    assert!((temp - 30.0).abs() < 1.0);
}

#[tokio::test]
async fn test_repeating_the_configuration_keeps_the_effect_of_actuators() {
    let settings = test_settings();
    let manager = PotManager::new(settings, WeatherClient::default());

    let mut manual = HashMap::new();
    manual.insert("soilMoisture".to_string(), 40.0);

    let config = PotConfig {
        crop_id: CROP.to_string(),
        key: KEY.to_string(),
        crop_type: "LETTUCE".to_string(),
        mode: "MANUAL".to_string(),
        manual: manual.clone(),
        location: None,
        interval_seconds: 30.0,
        managed: true,
        setting: None,
        exposure: None,
    };

    let pot = manager.upsert(config.clone()).await.unwrap();
    {
        let p = pot.read().await;
        let mut env = p.device.environment.write().await;
        env.state.insert("soilMoisture".to_string(), 70.0);
    }

    let _ = manager.upsert(config).await.unwrap();

    let moisture = {
        let p = pot.read().await;
        let env = p.device.environment.read().await;
        env.state.get("soilMoisture").copied().unwrap()
    };

    assert_eq!(moisture, 70.0);
}

#[tokio::test]
async fn test_control_api_requires_the_token() {
    let settings = test_settings();
    let manager = Arc::new(PotManager::new(settings, WeatherClient::default()));
    manager.load_static().await.unwrap();

    let state = AppState {
        manager: Arc::clone(&manager),
        api_token: Some(TOKEN.to_string()),
    };
    let app = create_router(state);

    // /health no requiere token
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["pots"], 1);

    // /v1/pots sin cabecera -> 401
    let req = Request::builder()
        .uri("/v1/pots")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // /v1/pots con token erróneo -> 401
    let req = Request::builder()
        .uri("/v1/pots")
        .header(header::AUTHORIZATION, "Bearer token-invalido")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // /v1/pots con token válido -> 200
    let req = Request::builder()
        .uri("/v1/pots")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_control_api_creates_updates_and_removes_a_pot() {
    let settings = test_settings();
    let manager = Arc::new(PotManager::new(settings, WeatherClient::default()));
    manager.load_static().await.unwrap();

    let state = AppState {
        manager: Arc::clone(&manager),
        api_token: Some(TOKEN.to_string()),
    };
    let app = create_router(state);

    // PUT crea cultivo virtual en modo AUTO
    let create_body = serde_json::json!({
        "key": KEY,
        "cropType": "LETTUCE",
        "mode": "AUTO",
        "intervalSeconds": 15
    });

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/pots/{CROP}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(create_body.to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["mode"], "AUTO");
    assert_eq!(body["managed"], true);

    // GET obtiene el cultivo creado
    let req = Request::builder()
        .uri(format!("/v1/pots/{CROP}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // DELETE elimina el cultivo creado -> 204
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/pots/{CROP}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // GET posterior -> 404
    let req = Request::builder()
        .uri(format!("/v1/pots/{CROP}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_without_token_the_control_api_is_disabled() {
    let mut settings = test_settings();
    settings.api_token = None;
    let manager = Arc::new(PotManager::new(settings, WeatherClient::default()));

    let state = AppState {
        manager: Arc::clone(&manager),
        api_token: None,
    };
    let app = create_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["api"], "DISABLED");

    let req = Request::builder()
        .uri("/v1/pots")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_static_pots_are_protected() {
    let settings = test_settings();
    let manager = Arc::new(PotManager::new(settings, WeatherClient::default()));
    manager.load_static().await.unwrap();

    let state = AppState {
        manager: Arc::clone(&manager),
        api_token: Some(TOKEN.to_string()),
    };
    let app = create_router(state);

    // Intento de borrar el cultivo estático 66f5a1000000000000000101 -> 409 Conflict
    let req = Request::builder()
        .method("DELETE")
        .uri("/v1/pots/66f5a1000000000000000101")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}
