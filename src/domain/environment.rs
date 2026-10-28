//! Modelo físico de simulación de un cultivo hidropónico.

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::PI;

use super::actuator::{readable_duration, Action, Actuator};
use super::crop::{baseline_for, Baseline};
use super::limits::clamp;
use super::weather::Weather;

const LIGHT_PER_WATT: f64 = 2.2;
const FAN_COOLING: f64 = 0.35;
const FAN_DRYING: f64 = 1.5;
const HUMIDIFIER_RATE: f64 = 2.5;
const LAMP_LIGHT: f64 = 900.0;
const LAMP_HEAT: f64 = 0.05;
const PUMP_RATE: f64 = 1.2;
const DOSE_SECONDS: u64 = 3;

fn sunlight(setting: &str, exposure: &str) -> f64 {
    match (setting, exposure) {
        ("OUTDOOR", "FULL_SUN") => 1.0,
        ("OUTDOOR", "PARTIAL_SUN") => 0.55,
        ("OUTDOOR", "SHADE") => 0.25,
        ("INDOOR", "FULL_SUN") => 0.45,
        ("INDOOR", "PARTIAL_SUN") => 0.25,
        ("INDOOR", "SHADE") => 0.08,
        _ => 1.0,
    }
}

fn daylight_factor(setting: &str, exposure: &str) -> f64 {
    match (setting, exposure) {
        ("OUTDOOR", "FULL_SUN") => 1.3,
        ("OUTDOOR", "PARTIAL_SUN") => 1.0,
        ("OUTDOOR", "SHADE") => 0.6,
        ("INDOOR", "FULL_SUN") => 0.9,
        ("INDOOR", "PARTIAL_SUN") => 0.7,
        ("INDOOR", "SHADE") => 0.35,
        _ => 1.0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Targets {
    pub temperature: f64,
    pub humidity: f64,
    pub brightness: f64,
    pub pressure: f64,
    pub drying: f64,
    pub rain: f64,
}

#[derive(Debug)]
pub struct Environment {
    pub crop_type: String,
    pub utc_offset_hours: f64,
    pub state: HashMap<String, f64>,
    pub active: HashMap<Actuator, (f64, Option<f64>)>,
    pub mode: String,
    pub manual: HashMap<String, f64>,
    pub weather: Option<Weather>,
    pub setting: Option<String>,
    pub exposure: Option<String>,
    base: Baseline,
    rng: StdRng,
}

impl Environment {
    pub fn new(crop_type: &str, seed: Option<u64>, utc_offset_hours: f64) -> Self {
        let base = baseline_for(crop_type);
        let mut state = HashMap::new();
        state.insert("temperature".to_string(), base.temperature);
        state.insert("humidity".to_string(), base.humidity);
        state.insert("brightness".to_string(), base.brightness);
        state.insert("ph".to_string(), base.ph);
        state.insert("tds".to_string(), base.tds);
        state.insert("soilMoisture".to_string(), base.soil_moisture);
        state.insert("atmosphere".to_string(), 1012.0);

        let manual = state.clone();

        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_entropy(),
        };

        Self {
            crop_type: crop_type.to_string(),
            utc_offset_hours,
            state,
            active: HashMap::new(),
            mode: "AUTO".to_string(),
            manual,
            weather: None,
            setting: None,
            exposure: None,
            base,
            rng,
        }
    }

    pub fn set_mode(&mut self, mode: &str) -> Result<(), String> {
        match mode {
            "AUTO" | "MANUAL" | "WEATHER" => {
                self.mode = mode.to_string();
                Ok(())
            }
            other => Err(format!("Modo desconocido: {other}")),
        }
    }

    pub fn set_placement(
        &mut self,
        setting: Option<String>,
        exposure: Option<String>,
    ) -> Result<(), String> {
        if let Some(ref s) = setting {
            if s != "INDOOR" && s != "OUTDOOR" {
                return Err(format!("Lugar desconocido: {s}"));
            }
        }
        if let Some(ref e) = exposure {
            if e != "FULL_SUN" && e != "PARTIAL_SUN" && e != "SHADE" {
                return Err(format!("Exposición desconocida: {e}"));
            }
        }
        self.setting = setting;
        self.exposure = exposure;
        Ok(())
    }

    pub fn set_manual(&mut self, values: &HashMap<String, f64>) {
        for (name, val) in values {
            let clamped = clamp(name, *val);
            self.manual.insert(name.clone(), clamped);
            self.state.insert(name.clone(), clamped);
        }
    }

    pub fn is_active(&self, actuator: Actuator, now: f64) -> bool {
        if let Some(&(start, stop)) = self.active.get(&actuator) {
            start <= now && stop.is_none_or(|end| now < end)
        } else {
            false
        }
    }

    pub fn active_actuators(&self, now: f64) -> HashMap<Actuator, Option<f64>> {
        let mut map = HashMap::new();
        for (&actuator, &(start, stop)) in &self.active {
            if start <= now && stop.is_none_or(|end| now < end) {
                map.insert(actuator, stop);
            }
        }
        map
    }

    pub fn active_fraction(&self, actuator: Actuator, start: f64, end: f64) -> f64 {
        if let Some(&(act_start, act_stop)) = self.active.get(&actuator) {
            if end <= start {
                return 0.0;
            }
            let stop = act_stop.map_or(end, |e| end.min(e));
            let overlap = (stop - start.max(act_start)).max(0.0);
            overlap / (end - start)
        } else {
            0.0
        }
    }

    pub fn apply(
        &mut self,
        actuator: Actuator,
        action: Action,
        duration: Option<u64>,
        now: f64,
    ) -> Result<String, String> {
        let (name, ending) = actuator.description();

        if action == Action::Deactivate {
            self.active.remove(&actuator);
            return Ok(format!("{name} apagad{ending}"));
        }

        let dur = if actuator.is_doser() {
            Some(duration.unwrap_or(DOSE_SECONDS))
        } else {
            duration
        };

        let end_time = dur.map(|d| now + d as f64);
        self.active.insert(actuator, (now, end_time));

        if actuator == Actuator::PhDoser {
            let current = self.state.get("ph").copied().unwrap_or(6.0);
            let d = dur.unwrap_or(DOSE_SECONDS) as f64;
            self.state
                .insert("ph".to_string(), clamp("ph", current - 0.12 * d));
        } else if actuator == Actuator::NutrientDoser {
            let current = self.state.get("tds").copied().unwrap_or(1000.0);
            let d = dur.unwrap_or(DOSE_SECONDS) as f64;
            self.state
                .insert("tds".to_string(), clamp("tds", current + 70.0 * d));
        }

        let msg = match dur {
            Some(d) if d > 0 => format!("{name} encendid{ending} por {}", readable_duration(d)),
            _ => format!("{name} encendid{ending}"),
        };

        Ok(msg)
    }

    pub fn step(&mut self, seconds: f64, now: f64) {
        let minutes = seconds / 60.0;
        let start = now - seconds;

        let fan = self.active_fraction(Actuator::Fan, start, now) * minutes;
        let humidifier = self.active_fraction(Actuator::Humidifier, start, now) * minutes;
        let uv_light = self.active_fraction(Actuator::UvLight, start, now);
        let pump_seconds = self.active_fraction(Actuator::WaterPump, start, now) * seconds;

        let targets = self.calc_targets(now);

        self.relax("temperature", targets.temperature, 0.08 * minutes);
        self.relax("humidity", targets.humidity, 0.1 * minutes);
        self.relax(
            "brightness",
            targets.brightness + LAMP_LIGHT * uv_light,
            2.0 * minutes,
        );
        self.relax("atmosphere", targets.pressure, 0.05 * minutes);

        self.push(
            "temperature",
            -FAN_COOLING * fan + LAMP_HEAT * uv_light * minutes,
        );
        self.push("humidity", -FAN_DRYING * fan + HUMIDIFIER_RATE * humidifier);

        let temp = self.state.get("temperature").copied().unwrap_or(20.0);
        let evaporation = 0.03 * minutes * (0.5f64).max(temp / 20.0) * targets.drying;
        let irrigation = PUMP_RATE * pump_seconds + targets.rain * minutes;

        let cur_moisture = self.state.get("soilMoisture").copied().unwrap_or(50.0);
        self.state.insert(
            "soilMoisture".to_string(),
            clamp("soilMoisture", cur_moisture - evaporation + irrigation),
        );

        if self.mode == "MANUAL" {
            let target_ph = self.manual.get("ph").copied().unwrap_or(6.0);
            let target_tds = self.manual.get("tds").copied().unwrap_or(1000.0);
            self.relax("ph", target_ph, 0.02 * minutes);
            self.relax("tds", target_tds, 0.02 * minutes);
        } else {
            let cur_ph = self.state.get("ph").copied().unwrap_or(6.0);
            let cur_tds = self.state.get("tds").copied().unwrap_or(1000.0);
            self.state
                .insert("ph".to_string(), clamp("ph", cur_ph + 0.002 * minutes));
            self.state
                .insert("tds".to_string(), clamp("tds", cur_tds - 0.4 * minutes));
        }
    }

    fn calc_targets(&self, now: f64) -> Targets {
        if self.mode == "MANUAL" {
            return Targets {
                temperature: self.manual.get("temperature").copied().unwrap_or(20.0),
                humidity: self.manual.get("humidity").copied().unwrap_or(60.0),
                brightness: self.manual.get("brightness").copied().unwrap_or(800.0),
                pressure: self.manual.get("atmosphere").copied().unwrap_or(1012.0),
                drying: 1.0,
                rain: 0.0,
            };
        }

        if self.mode == "WEATHER" {
            if let Some(ref w) = self.weather {
                let setting = self.setting.as_deref().unwrap_or("OUTDOOR");
                let exposure = self.exposure.as_deref().unwrap_or("FULL_SUN");
                let sun = sunlight(setting, exposure);
                let light = (w.radiation * LIGHT_PER_WATT * sun).min(2000.0);

                if setting == "INDOOR" {
                    let temperature =
                        21.0 + (w.temperature - 21.0) * 0.35 + 2.0 * sun * w.radiation / 1000.0;
                    let humidity = 55.0 + (w.humidity - 55.0) * 0.4;
                    return Targets {
                        temperature,
                        humidity,
                        brightness: light,
                        pressure: w.pressure,
                        drying: 0.8 * (1.0 + light / 1500.0),
                        rain: 0.0,
                    };
                }

                let temperature = w.temperature + 4.0 * sun * w.radiation / 1000.0;
                let drying =
                    (1.0 + sun * w.radiation / 600.0) * (0.3f64).max(1.3 - w.humidity / 100.0);
                let rain = w.precipitation * 0.8 / 60.0;
                return Targets {
                    temperature,
                    humidity: w.humidity,
                    brightness: light,
                    pressure: w.pressure,
                    drying,
                    rain,
                };
            }
        }

        // AUTO
        let hour = (now / 3600.0 + self.utc_offset_hours).rem_euclid(24.0);
        let daylight = ((hour - 6.0) / 12.0 * PI).sin();
        let factor = if let Some(ref s) = self.setting {
            daylight_factor(s, self.exposure.as_deref().unwrap_or("PARTIAL_SUN"))
        } else {
            1.0
        };

        Targets {
            temperature: self.base.temperature + 3.0 * daylight,
            humidity: self.base.humidity - 6.0 * daylight,
            brightness: self.base.brightness * factor * (0.05f64).max(daylight + 0.3),
            pressure: 1012.0,
            drying: 1.0,
            rain: 0.0,
        }
    }

    fn relax(&mut self, name: &str, target: f64, rate: f64) {
        if let Some(current) = self.state.get(name).copied() {
            let clamped = clamp(name, current + (target - current) * rate.min(1.0));
            self.state.insert(name.to_string(), clamped);
        }
    }

    fn push(&mut self, name: &str, delta: f64) {
        if let Some(current) = self.state.get(name).copied() {
            let clamped = clamp(name, current + delta);
            self.state.insert(name.to_string(), clamped);
        }
    }

    pub fn reading(&mut self) -> HashMap<String, f64> {
        let noise_levels = [
            ("temperature", 0.2, 1),
            ("humidity", 0.6, 1),
            ("brightness", 15.0, 0),
            ("ph", 0.03, 2),
            ("tds", 8.0, 0),
            ("soilMoisture", 0.4, 1),
            ("atmosphere", 0.4, 1),
        ];

        let mut res = HashMap::new();
        for (name, std_dev, decimals) in noise_levels {
            let val = self.state.get(name).copied().unwrap_or(0.0);
            let normal = Normal::new(0.0, std_dev).unwrap();
            let noise = normal.sample(&mut self.rng);
            let clamped = clamp(name, val + noise);
            let factor = 10.0f64.powi(decimals);
            let rounded = (clamped * factor).round() / factor;
            res.insert(name.to_string(), rounded);
        }

        res
    }
}
