//! Pruebas unitarias del dispositivo simulado y contrato MQTT v1.

use std::sync::Arc;
use tokio::sync::RwLock;

use smartpot_datagenerator::config::parse_devices;
use smartpot_datagenerator::device::{DeviceTopics, SimulatedDevice};
use smartpot_datagenerator::domain::environment::Environment;

const CROP: &str = "66f5a1000000000000000101";

#[test]
fn test_topics_follow_the_v1_contract() {
    let topics = DeviceTopics::new("smartpot/v1", CROP);
    assert_eq!(topics.telemetry, format!("smartpot/v1/{CROP}/telemetry"));
    assert_eq!(topics.commands, format!("smartpot/v1/{CROP}/commands"));
    assert_eq!(topics.ack, format!("smartpot/v1/{CROP}/commands/ack"));
    assert_eq!(topics.status, format!("smartpot/v1/{CROP}/status"));
}

#[tokio::test]
async fn test_commands_are_acknowledged_as_executed() {
    let env_arc = Arc::new(RwLock::new(Environment::new("LETTUCE", Some(1), 0.0)));
    let last_cmd_arc = Arc::new(RwLock::new(None));

    let payload =
        r#"{"id": "c1", "actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15}"#;
    let ack = SimulatedDevice::process_command(payload, 0.0, &env_arc, &last_cmd_arc, CROP)
        .await
        .unwrap();

    assert_eq!(ack.id, "c1");
    assert_eq!(ack.status, "EXECUTED");
    assert_eq!(ack.message, "Bomba de agua encendida por 15 s");
}

#[tokio::test]
async fn test_unsupported_actuators_fail_with_a_reason() {
    let env_arc = Arc::new(RwLock::new(Environment::new("LETTUCE", Some(1), 0.0)));
    let last_cmd_arc = Arc::new(RwLock::new(None));

    let payload = r#"{"id": "c2", "actuator": "HEATER", "action": "ACTIVATE"}"#;
    let ack = SimulatedDevice::process_command(payload, 0.0, &env_arc, &last_cmd_arc, CROP)
        .await
        .unwrap();

    assert_eq!(ack.id, "c2");
    assert_eq!(ack.status, "FAILED");
    assert!(ack.message.contains("HEATER"));
}

#[tokio::test]
async fn test_unreadable_commands_are_ignored() {
    let env_arc = Arc::new(RwLock::new(Environment::new("LETTUCE", Some(1), 0.0)));
    let last_cmd_arc = Arc::new(RwLock::new(None));

    assert!(
        SimulatedDevice::process_command("no es json", 0.0, &env_arc, &last_cmd_arc, CROP)
            .await
            .is_none()
    );

    assert!(SimulatedDevice::process_command(
        r#"{"actuator": "FAN"}"#,
        0.0,
        &env_arc,
        &last_cmd_arc,
        CROP
    )
    .await
    .is_none());
}

#[test]
fn test_device_list_parsing() {
    let raw = format!("{CROP}:k1:lettuce, 66f5a1000000000000000102:k2:TOMATO");
    let devices = parse_devices(&raw, true).unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].crop_type, "LETTUCE");
    assert_eq!(devices[1].crop_type, "TOMATO");

    assert!(parse_devices("sin-formato", true).is_err());
    assert!(parse_devices("", true).is_err());
    assert!(parse_devices("", false).unwrap().is_empty());
}
