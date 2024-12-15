use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Deserialize;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut mqttoptions = MqttOptions::new("rumqtt-doot-test", "100.92.108.55", 1883);
    mqttoptions.set_credentials("dan", "");
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (mut client, mut connection) = AsyncClient::new(mqttoptions, 10);
    client.subscribe("#", QoS::ExactlyOnce).await.unwrap();

    loop {
        let fuck = connection.poll().await;
        println!("{:#?}", fuck);
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    mqtt: MqttConfig,
}

#[derive(Debug, Deserialize)]
struct MqttConfig {
    host: String,
    port: u16,

    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct AirZoneConfig {
    name: String,
    filter: AirFilterConfig,
    presence: Vec<PresenceSensorConfig>,
    air_quality: Vec<AirQualitySensorConfig>,
}

#[derive(Debug, Deserialize)]
struct AirFilterConfig {
    name: String,
    command_topic: String,
}

#[derive(Debug, Deserialize)]
struct PresenceSensorConfig {
    name: String,
    state_topic: String,
}

#[derive(Debug, Deserialize)]
struct AirQualitySensorConfig {
    name: String,
    pm1: AirQualityMetricConfig,
    pm2_5: AirQualityMetricConfig,
    pm10: AirQualityMetricConfig,
}

#[derive(Debug, Deserialize)]
struct AirQualityMetricConfig {
    value_topic: String,
    dirty_threshold: f64,
    very_dirty_threshold: f64,
}

#[derive(Debug, Deserialize)]
enum AirCleanliness {
    Clean,
    Dirty,
    VeryDirty,
}
