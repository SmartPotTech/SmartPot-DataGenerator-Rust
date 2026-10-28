//! Límites de medición física de los sensores.

pub const LIMIT_TEMPERATURE: (f64, f64) = (-20.0, 60.0);
pub const LIMIT_HUMIDITY: (f64, f64) = (0.0, 100.0);
pub const LIMIT_BRIGHTNESS: (f64, f64) = (0.0, 2000.0);
pub const LIMIT_PH: (f64, f64) = (0.0, 14.0);
pub const LIMIT_TDS: (f64, f64) = (0.0, 3000.0);
pub const LIMIT_SOIL_MOISTURE: (f64, f64) = (0.0, 100.0);
pub const LIMIT_ATMOSPHERE: (f64, f64) = (300.0, 1100.0);

pub fn get_limits(name: &str) -> (f64, f64) {
    match name {
        "temperature" => LIMIT_TEMPERATURE,
        "humidity" => LIMIT_HUMIDITY,
        "brightness" => LIMIT_BRIGHTNESS,
        "ph" => LIMIT_PH,
        "tds" => LIMIT_TDS,
        "soilMoisture" | "soil_moisture" => LIMIT_SOIL_MOISTURE,
        "atmosphere" => LIMIT_ATMOSPHERE,
        _ => (-1e9, 1e9),
    }
}

#[inline]
pub fn clamp(name: &str, value: f64) -> f64 {
    let (low, high) = get_limits(name);
    value.clamp(low, high)
}
