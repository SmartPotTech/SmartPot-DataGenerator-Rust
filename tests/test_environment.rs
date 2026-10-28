//! Pruebas unitarias del modelo físico del cultivo hidropónico.

use smartpot_datagenerator::domain::actuator::{Action, Actuator};
use smartpot_datagenerator::domain::environment::Environment;
use smartpot_datagenerator::domain::limits::get_limits;
use smartpot_datagenerator::domain::weather::Weather;

const NOON: f64 = 12.0 * 3600.0;
const MIDNIGHT: f64 = 24.0 * 3600.0;

fn advance(env: &mut Environment, minutes: u32, start: f64, step: f64) -> f64 {
    let mut now = start;
    let steps = (minutes as f64 * 60.0 / step) as u32;
    for _ in 0..steps {
        now += step;
        env.step(step, now);
    }
    now
}

#[test]
fn test_readings_stay_inside_the_sensor_scale() {
    let mut env = Environment::new("TOMATO", Some(1), 0.0);
    advance(&mut env, 600, NOON, 30.0);
    let reading = env.reading();
    for (name, val) in reading {
        let (low, high) = get_limits(&name);
        assert!(
            val >= low && val <= high,
            "Sensor {name} fuera de escala: {val} no está en [{low}, {high}]"
        );
    }
}

#[test]
fn test_water_pump_raises_soil_moisture() {
    let mut env = Environment::new("LETTUCE", Some(1), 0.0);
    env.state.insert("soilMoisture".to_string(), 30.0);
    let _ = env.apply(Actuator::WaterPump, Action::Activate, Some(20), NOON);
    advance(&mut env, 1, NOON, 30.0);
    let moisture = env.state.get("soilMoisture").copied().unwrap();
    assert!(
        moisture > 45.0,
        "La humedad esperada > 45, obtenida: {moisture}"
    );
}

#[test]
fn test_fan_cools_the_air() {
    let mut with_fan = Environment::new("BASIL", Some(1), 0.0);
    let mut without_fan = Environment::new("BASIL", Some(1), 0.0);

    let _ = with_fan.apply(Actuator::Fan, Action::Activate, Some(3600), NOON);
    advance(&mut with_fan, 60, NOON, 30.0);
    advance(&mut without_fan, 60, NOON, 30.0);

    let temp_with = with_fan.state.get("temperature").copied().unwrap();
    let temp_without = without_fan.state.get("temperature").copied().unwrap();
    assert!(
        temp_with < temp_without,
        "{temp_with} no es menor que {temp_without}"
    );
}

#[test]
fn test_uv_light_adds_light_at_night() {
    let mut env = Environment::new("LETTUCE", Some(1), 0.0);
    advance(&mut env, 60, MIDNIGHT, 30.0);
    let dark = env.state.get("brightness").copied().unwrap();

    let _ = env.apply(
        Actuator::UvLight,
        Action::Activate,
        Some(3600),
        MIDNIGHT + 3600.0,
    );
    advance(&mut env, 30, MIDNIGHT + 3600.0, 30.0);
    let lit = env.state.get("brightness").copied().unwrap();
    assert!(
        lit > dark + 300.0,
        "Brillo lit ({lit}) debería ser > dark ({dark}) + 300"
    );
}

#[test]
fn test_ph_doser_lowers_ph_and_deactivate_stops_actuators() {
    let mut env = Environment::new("TOMATO", Some(1), 0.0);
    let before = env.state.get("ph").copied().unwrap();
    let _ = env.apply(Actuator::PhDoser, Action::Activate, Some(3), NOON);
    let after = env.state.get("ph").copied().unwrap();
    assert!((after - (before - 0.36)).abs() < 1e-4);

    let _ = env.apply(Actuator::Fan, Action::Activate, Some(600), NOON);
    let _ = env.apply(Actuator::Fan, Action::Deactivate, None, NOON + 1.0);
    assert!(!env.is_active(Actuator::Fan, NOON + 2.0));
}

#[test]
fn test_an_actuator_without_duration_stays_on_until_turned_off() {
    let mut env = Environment::new("BASIL", Some(1), 0.0);
    let msg = env
        .apply(Actuator::Fan, Action::Activate, None, NOON)
        .unwrap();
    assert_eq!(msg, "Ventilador encendido");
    assert!(env.is_active(Actuator::Fan, NOON + 5.0 * 3600.0));

    let active = env.active_actuators(NOON + 5.0 * 3600.0);
    assert_eq!(active.get(&Actuator::Fan), Some(&None));
    assert_eq!(
        env.active_fraction(Actuator::Fan, NOON + 3600.0, NOON + 3660.0),
        1.0
    );

    let _ = env.apply(Actuator::Fan, Action::Deactivate, None, NOON + 6.0 * 3600.0);
    assert!(!env.is_active(Actuator::Fan, NOON + 6.0 * 3600.0 + 1.0));
}

#[test]
fn test_acknowledgements_read_durations_like_a_person() {
    let mut env = Environment::new("TOMATO", Some(1), 0.0);
    assert_eq!(
        env.apply(Actuator::WaterPump, Action::Activate, Some(15), NOON)
            .unwrap(),
        "Bomba de agua encendida por 15 s"
    );
    assert_eq!(
        env.apply(Actuator::Fan, Action::Activate, Some(600), NOON)
            .unwrap(),
        "Ventilador encendido por 10 min"
    );
    assert_eq!(
        env.apply(Actuator::UvLight, Action::Activate, Some(7200), NOON)
            .unwrap(),
        "Luz ultravioleta encendida por 2 h"
    );
}

#[test]
fn test_a_doser_without_duration_releases_one_dose() {
    let mut env = Environment::new("TOMATO", Some(1), 0.0);
    let before = env.state.get("ph").copied().unwrap();
    let msg = env
        .apply(Actuator::PhDoser, Action::Activate, None, NOON)
        .unwrap();
    assert_eq!(msg, "Dosificador de pH encendido por 3 s");
    let after = env.state.get("ph").copied().unwrap();
    assert!((after - (before - 0.36)).abs() < 1e-4);
    assert!(!env.is_active(Actuator::PhDoser, NOON + 4.0));
}

#[test]
fn test_daylight_follows_the_local_time_zone() {
    let mut local = Environment::new("TOMATO", Some(1), -5.0);
    let mut utc = Environment::new("TOMATO", Some(1), 0.0);
    let five_pm_utc = 17.0 * 3600.0;
    advance(&mut local, 120, five_pm_utc, 30.0);
    advance(&mut utc, 120, five_pm_utc, 30.0);

    let b_local = local.state.get("brightness").copied().unwrap();
    let b_utc = utc.state.get("brightness").copied().unwrap();
    assert!(
        b_local > b_utc + 300.0,
        "{b_local} debería ser > {b_utc} + 300"
    );
}

#[test]
fn test_the_air_goes_back_when_the_fan_stops() {
    let mut env = Environment::new("BASIL", Some(1), 0.0);
    advance(&mut env, 30, NOON, 30.0);
    let ambient = env.state.get("temperature").copied().unwrap();

    let _ = env.apply(Actuator::Fan, Action::Activate, Some(600), NOON + 1800.0);
    advance(&mut env, 10, NOON + 1800.0, 30.0);
    let cooled = env.state.get("temperature").copied().unwrap();

    advance(&mut env, 60, NOON + 2400.0, 30.0);
    let recovered = env.state.get("temperature").copied().unwrap();
    assert!(cooled < recovered && (recovered - ambient).abs() < 1.5);
}

#[test]
fn test_placement_filters_the_weather() {
    let sunny = Weather {
        temperature: 30.0,
        humidity: 45.0,
        cloud_cover: 0.0,
        radiation: 900.0,
        precipitation: 0.0,
        pressure: 850.0,
        wind_speed: 5.0,
        is_day: true,
        code: 0,
        condition: "CLEAR".to_string(),
        label: "Despejado".to_string(),
        observed_at: "".to_string(),
    };

    let mut full_sun = Environment::new("TOMATO", Some(1), 0.0);
    let _ = full_sun.set_mode("WEATHER");
    let _ = full_sun.set_placement(Some("OUTDOOR".to_string()), Some("FULL_SUN".to_string()));
    full_sun.weather = Some(sunny.clone());
    advance(&mut full_sun, 180, NOON, 30.0);

    let mut shade = Environment::new("TOMATO", Some(1), 0.0);
    let _ = shade.set_mode("WEATHER");
    let _ = shade.set_placement(Some("OUTDOOR".to_string()), Some("SHADE".to_string()));
    shade.weather = Some(sunny.clone());
    advance(&mut shade, 180, NOON, 30.0);

    let mut indoor = Environment::new("TOMATO", Some(1), 0.0);
    let _ = indoor.set_mode("WEATHER");
    let _ = indoor.set_placement(Some("INDOOR".to_string()), Some("PARTIAL_SUN".to_string()));
    indoor.weather = Some(sunny);
    advance(&mut indoor, 180, NOON, 30.0);

    let sun_temp = full_sun.state.get("temperature").copied().unwrap();
    let shade_temp = shade.state.get("temperature").copied().unwrap();
    let indoor_temp = indoor.state.get("temperature").copied().unwrap();

    assert!(sun_temp > shade_temp);
    assert!(shade_temp > indoor_temp);
}
