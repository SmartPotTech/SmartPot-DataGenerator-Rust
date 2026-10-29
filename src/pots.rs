//! Administrador del ciclo de vida de los cultivos virtuales.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::{DeviceConfig, Settings};
use crate::device::SimulatedDevice;
use crate::domain::weather::WeatherClient;

const TICK_INTERVAL: Duration = Duration::from_millis(1000);
const WEATHER_RETRY_SECONDS: f64 = 120.0;
const AFTER_COMMAND_SECONDS: f64 = 2.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PotConfig {
    pub crop_id: String,
    pub key: String,
    pub crop_type: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub manual: HashMap<String, f64>,
    pub location: Option<Location>,
    #[serde(default = "default_interval")]
    pub interval_seconds: f64,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub setting: Option<String>,
    pub exposure: Option<String>,
}

fn default_mode() -> String {
    "AUTO".to_string()
}

fn default_interval() -> f64 {
    30.0
}

fn default_true() -> bool {
    true
}

pub struct VirtualPot {
    pub config: PotConfig,
    pub device: SimulatedDevice,
    pub next_at: f64,
    pub weather_error: Option<String>,
    pub weather_checked_at: f64,
}

impl VirtualPot {
    pub fn new(config: PotConfig, device: SimulatedDevice) -> Self {
        Self {
            config,
            device,
            next_at: 0.0,
            weather_error: None,
            weather_checked_at: 0.0,
        }
    }
}

pub struct PotManager {
    pub settings: Settings,
    pub weather_client: WeatherClient,
    pub pots: Arc<RwLock<HashMap<String, Arc<RwLock<VirtualPot>>>>>,
    tick_task: Mutex<Option<JoinHandle<()>>>,
}

impl PotManager {
    pub fn new(settings: Settings, weather_client: WeatherClient) -> Self {
        Self {
            settings,
            weather_client,
            pots: Arc::new(RwLock::new(HashMap::new())),
            tick_task: Mutex::new(None),
        }
    }

    pub async fn load_static(&self) -> Result<(), String> {
        for d in &self.settings.devices {
            let config = PotConfig {
                crop_id: d.crop_id.clone(),
                key: d.key.clone(),
                crop_type: d.crop_type.clone(),
                mode: "AUTO".to_string(),
                manual: HashMap::new(),
                location: None,
                interval_seconds: self.settings.interval_seconds,
                managed: false,
                setting: None,
                exposure: None,
            };
            self.upsert(config).await?;
        }
        Ok(())
    }

    pub async fn upsert(&self, config: PotConfig) -> Result<Arc<RwLock<VirtualPot>>, String> {
        if config.mode != "AUTO" && config.mode != "MANUAL" && config.mode != "WEATHER" {
            return Err(format!("Modo desconocido: {}", config.mode));
        }

        if config.mode == "WEATHER" && config.location.is_none() {
            return Err("El modo clima necesita una ubicación".to_string());
        }

        if let Some(ref s) = config.setting {
            if s != "INDOOR" && s != "OUTDOOR" {
                return Err(format!("Lugar desconocido: {s}"));
            }
        }

        if let Some(ref e) = config.exposure {
            if e != "FULL_SUN" && e != "PARTIAL_SUN" && e != "SHADE" {
                return Err(format!("Exposición desconocida: {e}"));
            }
        }

        let mut pots = self.pots.write().await;
        let now = Utc::now().timestamp() as f64;

        let (pot_entry, previous_config) = if let Some(existing_pot) = pots.get(&config.crop_id) {
            let previous = {
                let p = existing_pot.read().await;
                p.config.clone()
            };

            let need_new_device = previous.key != config.key
                || !previous.crop_type.eq_ignore_ascii_case(&config.crop_type);

            if need_new_device {
                {
                    let mut p = existing_pot.write().await;
                    p.device.stop().await;
                }

                let (tx, mut rx) = mpsc::channel::<()>(16);
                let target_crop_id = config.crop_id.clone();
                let pots_ref = Arc::clone(&self.pots);

                tokio::spawn(async move {
                    while rx.recv().await.is_some() {
                        let pots_map = pots_ref.read().await;
                        if let Some(target) = pots_map.get(&target_crop_id) {
                            let mut pot_guard = target.write().await;
                            let n = Utc::now().timestamp() as f64;
                            pot_guard.next_at = if pot_guard.next_at > 0.0 {
                                pot_guard.next_at.min(n + AFTER_COMMAND_SECONDS)
                            } else {
                                n + AFTER_COMMAND_SECONDS
                            };
                        }
                    }
                });

                let mut device = SimulatedDevice::new(
                    DeviceConfig {
                        crop_id: config.crop_id.clone(),
                        key: config.key.clone(),
                        crop_type: config.crop_type.clone(),
                    },
                    &self.settings,
                    Some(tx),
                );
                device.start(&self.settings);

                let pot = Arc::new(RwLock::new(VirtualPot::new(config.clone(), device)));
                pots.insert(config.crop_id.clone(), Arc::clone(&pot));
                info!(
                    "Cultivo virtual {} ({}) en modo {}",
                    config.crop_id, config.crop_type, config.mode
                );
                (pot, None)
            } else {
                (Arc::clone(existing_pot), Some(previous))
            }
        } else {
            let (tx, mut rx) = mpsc::channel::<()>(16);
            let target_crop_id = config.crop_id.clone();
            let pots_ref = Arc::clone(&self.pots);

            tokio::spawn(async move {
                while rx.recv().await.is_some() {
                    let pots_map = pots_ref.read().await;
                    if let Some(target) = pots_map.get(&target_crop_id) {
                        let mut pot_guard = target.write().await;
                        let n = Utc::now().timestamp() as f64;
                        pot_guard.next_at = if pot_guard.next_at > 0.0 {
                            pot_guard.next_at.min(n + AFTER_COMMAND_SECONDS)
                        } else {
                            n + AFTER_COMMAND_SECONDS
                        };
                    }
                }
            });

            let mut device = SimulatedDevice::new(
                DeviceConfig {
                    crop_id: config.crop_id.clone(),
                    key: config.key.clone(),
                    crop_type: config.crop_type.clone(),
                },
                &self.settings,
                Some(tx),
            );
            device.start(&self.settings);

            let pot = Arc::new(RwLock::new(VirtualPot::new(config.clone(), device)));
            pots.insert(config.crop_id.clone(), Arc::clone(&pot));
            info!(
                "Cultivo virtual {} ({}) en modo {}",
                config.crop_id, config.crop_type, config.mode
            );
            (pot, None)
        };

        // Actualizamos configuración y entorno
        {
            let mut pot_guard = pot_entry.write().await;
            pot_guard.config = config.clone();

            {
                let mut env = pot_guard.device.environment.write().await;
                let _ = env.set_mode(&config.mode);
                let _ = env.set_placement(config.setting.clone(), config.exposure.clone());

                let mut changed = HashMap::new();
                for (k, v) in &config.manual {
                    let prev_val = previous_config.as_ref().and_then(|p| p.manual.get(k));
                    if previous_config.is_none() || prev_val != Some(v) {
                        changed.insert(k.clone(), *v);
                    }
                }
                if !changed.is_empty() {
                    env.set_manual(&changed);
                }
            }

            let location_changed =
                previous_config.as_ref().map(|p| &p.location) != Some(&config.location);
            let needs_weather = {
                let env = pot_guard.device.environment.read().await;
                env.weather.is_none()
            };

            if config.mode == "WEATHER" && (location_changed || needs_weather) {
                Self::refresh_weather(&self.weather_client, &mut pot_guard, true).await;
            }

            pot_guard.next_at = if pot_guard.next_at > 0.0 {
                pot_guard.next_at.min(now + 2.0)
            } else {
                now + 2.0
            };
        }

        Ok(pot_entry)
    }

    pub async fn remove(&self, crop_id: &str) -> bool {
        let mut pots = self.pots.write().await;
        if let Some(pot) = pots.remove(crop_id) {
            let mut p = pot.write().await;
            p.device.stop().await;
            info!("Cultivo virtual {crop_id} retirado");
            true
        } else {
            false
        }
    }

    pub async fn get(&self, crop_id: &str) -> Option<Arc<RwLock<VirtualPot>>> {
        let pots = self.pots.read().await;
        pots.get(crop_id).cloned()
    }

    pub async fn all(&self) -> Vec<Arc<RwLock<VirtualPot>>> {
        let pots = self.pots.read().await;
        pots.values().cloned().collect()
    }

    pub async fn refresh_weather(client: &WeatherClient, pot: &mut VirtualPot, force: bool) {
        let loc = match &pot.config.location {
            Some(l) => l,
            None => return,
        };

        let now = Utc::now().timestamp() as f64;
        if !force
            && pot.weather_error.is_some()
            && now - pot.weather_checked_at < WEATHER_RETRY_SECONDS
        {
            return;
        }

        pot.weather_checked_at = now;
        match client.current(loc.latitude, loc.longitude).await {
            Ok(weather) => {
                let mut env = pot.device.environment.write().await;
                env.weather = Some(weather);
                pot.weather_error = None;
            }
            Err(e) => {
                pot.weather_error = Some(
                    "No se pudo consultar el clima; se usa el último dato conocido.".to_string(),
                );
                warn!("Clima no disponible para {}: {}", loc.name, e);
            }
        }
    }

    pub async fn tick(&self, now: f64) {
        let all_pots = self.all().await;
        let mut tasks = Vec::new();

        for pot_arc in all_pots {
            let should_run = {
                let pot = pot_arc.read().await;
                now >= pot.next_at
            };

            if should_run {
                let client = self.weather_client.clone();
                let pot_clone = Arc::clone(&pot_arc);
                tasks.push(tokio::spawn(async move {
                    let mut pot = pot_clone.write().await;
                    pot.next_at = now + pot.config.interval_seconds;

                    if pot.config.mode == "WEATHER" {
                        Self::refresh_weather(&client, &mut pot, false).await;
                    }

                    pot.device.publish_reading(now).await;
                }));
            }
        }

        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn start(&self) {
        let mut task_guard = self.tick_task.lock().await;
        if task_guard.is_some() {
            return;
        }

        let pots_ref = Arc::clone(&self.pots);
        let weather_client = self.weather_client.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(TICK_INTERVAL);
            loop {
                interval.tick().await;
                let now = Utc::now().timestamp() as f64;

                let all_pots: Vec<Arc<RwLock<VirtualPot>>> = {
                    let pots = pots_ref.read().await;
                    pots.values().cloned().collect()
                };

                let mut tasks = Vec::new();
                for pot_arc in all_pots {
                    let should_run = {
                        let pot = pot_arc.read().await;
                        now >= pot.next_at
                    };

                    if should_run {
                        let client = weather_client.clone();
                        let pot_clone = Arc::clone(&pot_arc);
                        tasks.push(tokio::spawn(async move {
                            let mut pot = pot_clone.write().await;
                            pot.next_at = now + pot.config.interval_seconds;

                            if pot.config.mode == "WEATHER" {
                                Self::refresh_weather(&client, &mut pot, false).await;
                            }

                            pot.device.publish_reading(now).await;
                        }));
                    }
                }

                for task in tasks {
                    let _ = task.await;
                }
            }
        });

        *task_guard = Some(handle);
    }

    pub async fn stop(&self) {
        let mut task_guard = self.tick_task.lock().await;
        if let Some(handle) = task_guard.take() {
            handle.abort();
        }

        let all_pots = self.all().await;
        for pot_arc in all_pots {
            let mut pot = pot_arc.write().await;
            pot.device.stop().await;
        }
    }

    pub async fn snapshot(
        &self,
        pot_arc: &Arc<RwLock<VirtualPot>>,
        now: Option<f64>,
    ) -> serde_json::Value {
        let now = now.unwrap_or_else(|| Utc::now().timestamp() as f64);
        let pot = pot_arc.read().await;
        let env = pot.device.environment.read().await;

        let last_reading = pot.device.last_reading.read().await.clone();
        let last_pub = pot.device.last_published_at.read().await.map(iso_time);
        let last_cmd = pot.device.last_command.read().await.clone().map(|cmd| {
            serde_json::json!({
                "id": cmd.id,
                "status": cmd.status,
                "message": cmd.message,
                "at": iso_time(cmd.at)
            })
        });

        let mut manual_rounded = serde_json::Map::new();
        for (k, v) in &env.manual {
            let rounded = (v * 100.0).round() / 100.0;
            manual_rounded.insert(k.clone(), serde_json::json!(rounded));
        }

        let active_actuators: Vec<serde_json::Value> = env
            .active_actuators(now)
            .into_iter()
            .map(|(act, until)| {
                serde_json::json!({
                    "actuator": act.as_str(),
                    "until": until.map(iso_time)
                })
            })
            .collect();

        serde_json::json!({
            "cropId": pot.config.crop_id,
            "cropType": pot.config.crop_type,
            "mode": pot.config.mode,
            "managed": pot.config.managed,
            "connected": pot.device.is_connected(),
            "intervalSeconds": pot.config.interval_seconds,
            "setting": pot.config.setting,
            "exposure": pot.config.exposure,
            "lastReading": last_reading,
            "lastPublishedAt": last_pub,
            "manual": manual_rounded,
            "location": pot.config.location,
            "weather": env.weather,
            "weatherError": pot.weather_error,
            "activeActuators": active_actuators,
            "lastCommand": last_cmd,
        })
    }
}

pub fn iso_time(epoch: f64) -> String {
    let secs = epoch.floor() as i64;
    let nsecs = ((epoch - epoch.floor()) * 1e9) as u32;
    DateTime::from_timestamp(secs, nsecs)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}
