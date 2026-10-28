//! Configuración del simulador cargada desde variables de entorno.

use std::env;

/// Configuración de un dispositivo estático definido por variables de entorno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceConfig {
    pub crop_id: String,
    pub key: String,
    pub crop_type: String,
}

/// Ajustes globales del servicio.
#[derive(Debug, Clone)]
pub struct Settings {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub ca_file: Option<String>,
    pub topic_prefix: String,
    pub interval_seconds: f64,
    pub devices: Vec<DeviceConfig>,
    pub utc_offset_hours: f64,
    pub api_token: Option<String>,
    pub api_port: u16,
}

/// Parsea la lista de dispositivos desde el string de configuración:
/// Formato: `cropId:clave:TIPO` separados por comas.
pub fn parse_devices(raw: &str, required: bool) -> Result<Vec<DeviceConfig>, String> {
    let mut devices = Vec::new();

    for chunk in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = chunk.split(':').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.trim().is_empty()) {
            return Err(format!(
                "Dispositivo mal definido: «{chunk}». Usa cropId:clave:TIPO"
            ));
        }
        devices.push(DeviceConfig {
            crop_id: parts[0].trim().to_string(),
            key: parts[1].trim().to_string(),
            crop_type: parts[2].trim().to_uppercase(),
        });
    }

    if devices.is_empty() && required {
        return Err(
            "Define al menos un dispositivo en SIMULATOR_DEVICES o activa la API con SIMULATOR_TOKEN"
                .to_string(),
        );
    }

    Ok(devices)
}

/// Carga la configuración desde las variables de entorno.
pub fn load() -> Result<Settings, String> {
    let tls = env::var("MQTT_TLS")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let token = env::var("SIMULATOR_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(ref tok) = token {
        if tok.len() < 24 {
            return Err("SIMULATOR_TOKEN debe tener al menos 24 caracteres".to_string());
        }
    }

    let default_port = if tls { 8883 } else { 1883 };
    let port = env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(default_port);

    let host = env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".to_string());
    let ca_file = env::var("MQTT_CA_FILE")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or_else(|_| "smartpot/v1".to_string());

    let interval_seconds = env::var("SIMULATOR_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30.0);

    let utc_offset_hours = env::var("SIMULATOR_UTC_OFFSET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-5.0);

    let api_port = env::var("SIMULATOR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);

    let raw_devices = env::var("SIMULATOR_DEVICES").unwrap_or_default();
    let devices = parse_devices(&raw_devices, token.is_none())?;

    Ok(Settings {
        host,
        port,
        tls,
        ca_file,
        topic_prefix,
        interval_seconds,
        devices,
        utc_offset_hours,
        api_token: token,
        api_port,
    })
}
