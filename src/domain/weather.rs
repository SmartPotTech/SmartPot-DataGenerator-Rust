//! Cliente del servicio de clima Open-Meteo con caché por coordenadas.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const WEATHER_URL: &str = "https://api.open-meteo.com/v1/forecast";
const GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const TIMEOUT: Duration = Duration::from_secs(8);
const USER_AGENT: &str =
    "SmartPot-Simulator/2.0 (+https://github.com/SmartPotTech/SmartPot-DataGenerator)";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub temperature: f64,
    pub humidity: f64,
    pub cloud_cover: f64,
    pub radiation: f64,
    pub precipitation: f64,
    pub pressure: f64,
    pub wind_speed: f64,
    pub is_day: bool,
    pub code: i32,
    pub condition: String,
    pub label: String,
    pub observed_at: String,
}

/// Mapea los códigos WMO al identificador de escena y texto en español.
pub fn condition(code: i32) -> (&'static str, &'static str) {
    match code {
        0 => ("CLEAR", "Despejado"),
        1 => ("MOSTLY_CLEAR", "Mayormente despejado"),
        2 => ("PARTLY_CLOUDY", "Parcialmente nublado"),
        3 => ("CLOUDY", "Nublado"),
        45 | 48 => ("FOG", "Niebla"),
        51 | 53 | 55 | 56 | 57 => ("DRIZZLE", "Llovizna"),
        61 | 63 | 65 | 66 | 67 | 80 | 81 | 82 => ("RAIN", "Lluvia"),
        71 | 73 | 75 | 77 | 85 | 86 => ("SNOW", "Nieve"),
        95 | 96 | 99 => ("STORM", "Tormenta"),
        _ => ("CLOUDY", "Nublado"),
    }
}

#[derive(Debug, Clone)]
struct CachedEntry {
    weather: Weather,
    fetched_at: Instant,
}

#[derive(Debug, Clone)]
pub struct WeatherClient {
    client: reqwest::Client,
    ttl: Duration,
    cache: Arc<RwLock<HashMap<(i64, i64), CachedEntry>>>,
}

impl Default for WeatherClient {
    fn default() -> Self {
        Self::new(Duration::from_secs(600))
    }
}

impl WeatherClient {
    pub fn new(ttl: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default();

        Self {
            client,
            ttl,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Redondea las coordenadas a 2 decimales para la clave de caché.
    fn cache_key(lat: f64, lon: f64) -> (i64, i64) {
        ((lat * 100.0).round() as i64, (lon * 100.0).round() as i64)
    }

    /// Consulta el clima actual para las coordenadas dadas.
    pub async fn current(&self, latitude: f64, longitude: f64) -> Result<Weather, String> {
        let key = Self::cache_key(latitude, longitude);

        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&key) {
                if entry.fetched_at.elapsed() < self.ttl {
                    return Ok(entry.weather.clone());
                }
            }
        }

        let lat_rounded = (latitude * 100.0).round() / 100.0;
        let lon_rounded = (longitude * 100.0).round() / 100.0;

        let url = format!(
            "{}?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,is_day,precipitation,weather_code,cloud_cover,surface_pressure,wind_speed_10m,shortwave_radiation&timezone=auto",
            WEATHER_URL, lat_rounded, lon_rounded
        );

        let res = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Error en petición a Open-Meteo: {e}"))?;

        if !res.status().is_success() {
            return Err(format!("Open-Meteo respondió código HTTP {}", res.status()));
        }

        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Error deserializando JSON de Open-Meteo: {e}"))?;

        let current = body
            .get("current")
            .ok_or_else(|| "Respuesta de Open-Meteo no contiene campo 'current'".to_string())?;

        let code = current
            .get("weather_code")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let (cond, label) = condition(code);

        let weather = Weather {
            temperature: current
                .get("temperature_2m")
                .and_then(|v| v.as_f64())
                .unwrap_or(20.0),
            humidity: current
                .get("relative_humidity_2m")
                .and_then(|v| v.as_f64())
                .unwrap_or(50.0),
            cloud_cover: current
                .get("cloud_cover")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            radiation: current
                .get("shortwave_radiation")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            precipitation: current
                .get("precipitation")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            pressure: current
                .get("surface_pressure")
                .and_then(|v| v.as_f64())
                .unwrap_or(1013.0),
            wind_speed: current
                .get("wind_speed_10m")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            is_day: current
                .get("is_day")
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(true),
            code,
            condition: cond.to_string(),
            label: label.to_string(),
            observed_at: current
                .get("time")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        };

        {
            let mut cache = self.cache.write().await;
            cache.insert(
                key,
                CachedEntry {
                    weather: weather.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(weather)
    }

    /// Busca lugares geográficos por nombre.
    pub async fn search(&self, query: &str, count: u8) -> Result<Vec<Place>, String> {
        let url = format!(
            "{}?name={}&count={}&language=es&format=json",
            GEOCODING_URL,
            urlencoding_simple(query),
            count
        );

        let res = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Error en geocoding de Open-Meteo: {e}"))?;

        if !res.status().is_success() {
            return Err(format!("Geocoding respondió código HTTP {}", res.status()));
        }

        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Error deserializando geocoding: {e}"))?;

        let mut places = Vec::new();
        if let Some(results) = body.get("results").and_then(|v| v.as_array()) {
            for item in results {
                if let (Some(name), Some(lat), Some(lon)) = (
                    item.get("name").and_then(|v| v.as_str()),
                    item.get("latitude").and_then(|v| v.as_f64()),
                    item.get("longitude").and_then(|v| v.as_f64()),
                ) {
                    places.push(Place {
                        name: name.to_string(),
                        latitude: (lat * 10000.0).round() / 10000.0,
                        longitude: (lon * 10000.0).round() / 10000.0,
                        country: item
                            .get("country")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        region: item
                            .get("admin1")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }

        Ok(places)
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*b as char);
            }
            b' ' => encoded.push('+'),
            other => {
                encoded.push_str(&format!("%{:02X}", other));
            }
        }
    }
    encoded
}
