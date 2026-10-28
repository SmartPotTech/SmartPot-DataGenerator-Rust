//! Perfiles de especies hidropónicas y valores basales.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub temperature: f64,
    pub humidity: f64,
    pub brightness: f64,
    pub ph: f64,
    pub tds: f64,
    pub soil_moisture: f64,
}

pub fn baseline_for(crop_type: &str) -> Baseline {
    match crop_type.to_ascii_uppercase().as_str() {
        "TOMATO" => Baseline {
            temperature: 25.0,
            humidity: 68.0,
            brightness: 1300.0,
            ph: 6.1,
            tds: 2000.0,
            soil_moisture: 64.0,
        },
        "STRAWBERRY" => Baseline {
            temperature: 22.0,
            humidity: 66.0,
            brightness: 1100.0,
            ph: 5.9,
            tds: 840.0,
            soil_moisture: 70.0,
        },
        "BASIL" => Baseline {
            temperature: 25.0,
            humidity: 50.0,
            brightness: 1100.0,
            ph: 6.0,
            tds: 900.0,
            soil_moisture: 60.0,
        },
        "SPINACH" => Baseline {
            temperature: 19.0,
            humidity: 60.0,
            brightness: 850.0,
            ph: 6.5,
            tds: 1400.0,
            soil_moisture: 70.0,
        },
        "PEPPER" => Baseline {
            temperature: 25.0,
            humidity: 60.0,
            brightness: 1300.0,
            ph: 6.1,
            tds: 1750.0,
            soil_moisture: 64.0,
        },
        _ => Baseline {
            // LETTUCE por defecto
            temperature: 19.0,
            humidity: 62.0,
            brightness: 850.0,
            ph: 6.0,
            tds: 700.0,
            soil_moisture: 70.0,
        },
    }
}
