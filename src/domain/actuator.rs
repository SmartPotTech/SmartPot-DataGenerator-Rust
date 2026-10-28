//! Definición y comportamiento de los actuadores del cultivo.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Actuator {
    WaterPump,
    UvLight,
    Fan,
    Humidifier,
    NutrientDoser,
    PhDoser,
}

impl Actuator {
    /// Devuelve el identificador estándar en mayúsculas.
    pub fn as_str(&self) -> &'static str {
        match self {
            Actuator::WaterPump => "WATER_PUMP",
            Actuator::UvLight => "UV_LIGHT",
            Actuator::Fan => "FAN",
            Actuator::Humidifier => "HUMIDIFIER",
            Actuator::NutrientDoser => "NUTRIENT_DOSER",
            Actuator::PhDoser => "PH_DOSER",
        }
    }

    /// Nombre amigable en español y desinencia de género ("a" u "o").
    pub fn description(&self) -> (&'static str, &'static str) {
        match self {
            Actuator::WaterPump => ("Bomba de agua", "a"),
            Actuator::UvLight => ("Luz ultravioleta", "a"),
            Actuator::Fan => ("Ventilador", "o"),
            Actuator::Humidifier => ("Humidificador", "o"),
            Actuator::NutrientDoser => ("Dosificador de nutrientes", "o"),
            Actuator::PhDoser => ("Dosificador de pH", "o"),
        }
    }

    /// Indica si es un dosificador (duración por defecto 3 s).
    pub fn is_doser(&self) -> bool {
        matches!(self, Actuator::NutrientDoser | Actuator::PhDoser)
    }
}

impl fmt::Display for Actuator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Actuator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "WATER_PUMP" => Ok(Actuator::WaterPump),
            "UV_LIGHT" => Ok(Actuator::UvLight),
            "FAN" => Ok(Actuator::Fan),
            "HUMIDIFIER" => Ok(Actuator::Humidifier),
            "NUTRIENT_DOSER" => Ok(Actuator::NutrientDoser),
            "PH_DOSER" => Ok(Actuator::PhDoser),
            other => Err(format!("El actuador {other} no existe en este cultivo")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Activate,
    Deactivate,
}

impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ACTIVATE" => Ok(Action::Activate),
            "DEACTIVATE" => Ok(Action::Deactivate),
            other => Err(format!("Acción desconocida: {other}")),
        }
    }
}

/// Formatea una duración en segundos como texto amigable: "15 s", "10 min", "2 h".
pub fn readable_duration(seconds: u64) -> String {
    if seconds.is_multiple_of(3600) {
        format!("{} h", seconds / 3600)
    } else if seconds.is_multiple_of(60) {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}
