//! API HTTP de control interno de los cultivos virtuales con Axum.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::pots::{Location, PotConfig, PotManager};

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<PotManager>,
    pub api_token: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(&'static str),
    NotFound(&'static str),
    BadRequest(String),
    Conflict(&'static str),
    ServiceUnavailable(&'static str),
    BadGateway(&'static str),
    UnprocessableEntity(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, detail, extra_header) = match self {
            ApiError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                msg.to_string(),
                Some((header::WWW_AUTHENTICATE, "Bearer")),
            ),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.to_string(), None),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg, None),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.to_string(), None),
            ApiError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg.to_string(), None)
            }
            ApiError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, msg.to_string(), None),
            ApiError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg, None),
        };

        let body = Json(serde_json::json!({ "detail": detail }));
        let mut resp = (status, body).into_response();
        if let Some((name, val)) = extra_header {
            if let Ok(v) = val.parse() {
                resp.headers_mut().insert(name, v);
            }
        }
        resp
    }
}

fn verify_token(headers: &HeaderMap, token_opt: &Option<String>) -> Result<(), ApiError> {
    let expected = match token_opt {
        Some(t) => t,
        None => {
            return Err(ApiError::ServiceUnavailable(
                "La API de control está deshabilitada",
            ))
        }
    };

    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") => {
            let provided = auth["Bearer ".len()..].trim();
            if provided.as_bytes().ct_eq(expected.as_bytes()).into() {
                Ok(())
            } else {
                Err(ApiError::Unauthorized("Token de servicio inválido"))
            }
        }
        _ => Err(ApiError::Unauthorized("Token de servicio inválido")),
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/v1/pots", get(list_pots_handler))
        .route("/v1/pots/{crop_id}", get(get_pot_handler))
        .route("/v1/pots/{crop_id}", put(put_pot_handler))
        .route("/v1/pots/{crop_id}", delete(delete_pot_handler))
        .route("/v1/places", get(places_handler))
        .route("/v1/weather", get(weather_handler))
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let all = state.manager.all().await;
    let mut connected_count = 0;
    for pot_arc in &all {
        let pot = pot_arc.read().await;
        if pot.device.is_connected() {
            connected_count += 1;
        }
    }

    let api_status = if state.api_token.is_some() {
        "ENABLED"
    } else {
        "DISABLED"
    };

    Json(serde_json::json!({
        "status": "UP",
        "pots": all.len(),
        "connected": connected_count,
        "api": api_status,
    }))
}

async fn list_pots_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    let all = state.manager.all().await;
    let mut snapshots = Vec::new();
    for pot_arc in &all {
        snapshots.push(state.manager.snapshot(pot_arc, None).await);
    }

    Ok(Json(snapshots))
}

async fn get_pot_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(crop_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    if let Some(pot) = state.manager.get(&crop_id).await {
        let snap = state.manager.snapshot(&pot, None).await;
        Ok(Json(snap))
    } else {
        Err(ApiError::NotFound("El cultivo virtual no existe"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualValuesInput {
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub brightness: Option<f64>,
    pub ph: Option<f64>,
    pub tds: Option<f64>,
    pub atmosphere: Option<f64>,
    pub soil_moisture: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInput {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PotRequest {
    pub key: String,
    pub crop_type: String,
    #[serde(default = "default_weather_mode")]
    pub mode: String,
    pub manual: Option<ManualValuesInput>,
    pub location: Option<LocationInput>,
    #[serde(default = "default_interval")]
    pub interval_seconds: f64,
    pub setting: Option<String>,
    pub exposure: Option<String>,
}

fn default_weather_mode() -> String {
    "WEATHER".to_string()
}

fn default_interval() -> f64 {
    30.0
}

async fn put_pot_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(crop_id): Path<String>,
    Json(payload): Json<PotRequest>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    if let Some(existing) = state.manager.get(&crop_id).await {
        let pot = existing.read().await;
        if !pot.config.managed {
            return Err(ApiError::Conflict(
                "Ese cultivo está definido en SIMULATOR_DEVICES",
            ));
        }
    }

    if payload.key.len() < 16 || payload.key.len() > 128 {
        return Err(ApiError::UnprocessableEntity(
            "La clave debe tener entre 16 y 128 caracteres".to_string(),
        ));
    }

    let crop_type_regex = Regex::new(r"^[A-Z_]{3,20}$").unwrap();
    if !crop_type_regex.is_match(&payload.crop_type) {
        return Err(ApiError::UnprocessableEntity(
            "cropType debe contener entre 3 y 20 mayúsculas o guiones bajos".to_string(),
        ));
    }

    if payload.interval_seconds < 10.0 || payload.interval_seconds > 300.0 {
        return Err(ApiError::UnprocessableEntity(
            "intervalSeconds debe estar entre 10 y 300".to_string(),
        ));
    }

    if let Some(ref s) = payload.setting {
        if s != "INDOOR" && s != "OUTDOOR" {
            return Err(ApiError::UnprocessableEntity(
                "setting debe ser INDOOR u OUTDOOR".to_string(),
            ));
        }
    }

    if let Some(ref e) = payload.exposure {
        if e != "FULL_SUN" && e != "PARTIAL_SUN" && e != "SHADE" {
            return Err(ApiError::UnprocessableEntity(
                "exposure debe ser FULL_SUN, PARTIAL_SUN o SHADE".to_string(),
            ));
        }
    }

    let mut manual_map = HashMap::new();
    if let Some(m) = payload.manual {
        if let Some(v) = m.temperature {
            manual_map.insert("temperature".to_string(), v);
        }
        if let Some(v) = m.humidity {
            manual_map.insert("humidity".to_string(), v);
        }
        if let Some(v) = m.brightness {
            manual_map.insert("brightness".to_string(), v);
        }
        if let Some(v) = m.ph {
            manual_map.insert("ph".to_string(), v);
        }
        if let Some(v) = m.tds {
            manual_map.insert("tds".to_string(), v);
        }
        if let Some(v) = m.atmosphere {
            manual_map.insert("atmosphere".to_string(), v);
        }
        if let Some(v) = m.soil_moisture {
            manual_map.insert("soilMoisture".to_string(), v);
        }
    }

    let location = payload.location.map(|l| Location {
        name: l.name,
        latitude: l.latitude,
        longitude: l.longitude,
    });

    let config = PotConfig {
        crop_id: crop_id.clone(),
        key: payload.key,
        crop_type: payload.crop_type,
        mode: payload.mode,
        manual: manual_map,
        location,
        interval_seconds: payload.interval_seconds,
        managed: true,
        setting: payload.setting,
        exposure: payload.exposure,
    };

    match state.manager.upsert(config).await {
        Ok(pot) => {
            let snap = state.manager.snapshot(&pot, None).await;
            Ok(Json(snap))
        }
        Err(e) => Err(ApiError::BadRequest(e)),
    }
}

async fn delete_pot_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(crop_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    if let Some(existing) = state.manager.get(&crop_id).await {
        let pot = existing.read().await;
        if !pot.config.managed {
            return Err(ApiError::Conflict(
                "Ese cultivo está definido en SIMULATOR_DEVICES",
            ));
        }
    }

    state.manager.remove(&crop_id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct PlacesQuery {
    pub q: String,
}

async fn places_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<PlacesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    if query.q.trim().len() < 2 || query.q.len() > 80 {
        return Err(ApiError::UnprocessableEntity(
            "El parámetro 'q' debe tener entre 2 y 80 caracteres".to_string(),
        ));
    }

    match state.manager.weather_client.search(&query.q, 5).await {
        Ok(places) => Ok(Json(places)),
        Err(_) => Err(ApiError::BadGateway(
            "No se pudo consultar el buscador de lugares",
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct WeatherQuery {
    pub latitude: f64,
    pub longitude: f64,
}

async fn weather_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<WeatherQuery>,
) -> Result<impl IntoResponse, ApiError> {
    verify_token(&headers, &state.api_token)?;

    if query.latitude < -90.0
        || query.latitude > 90.0
        || query.longitude < -180.0
        || query.longitude > 180.0
    {
        return Err(ApiError::UnprocessableEntity(
            "Coordenadas fuera de rango válido".to_string(),
        ));
    }

    match state
        .manager
        .weather_client
        .current(query.latitude, query.longitude)
        .await
    {
        Ok(w) => Ok(Json(w)),
        Err(_) => Err(ApiError::BadGateway("No se pudo consultar el clima")),
    }
}
