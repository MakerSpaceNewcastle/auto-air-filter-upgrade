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
    presence: Vec<PresenceSensor>,
    air_quality: Vec<AirQualitySensor>,
}

#[derive(Debug, Deserialize)]
struct AirFilterConfig {
    name: String,
    command_topic: String,
}

#[derive(Debug, Deserialize)]
struct PresenceSensor {
    name: String,
    state_topic: String,
}

#[derive(Debug, Deserialize)]
struct AirQualitySensor {
    name: String,
    pm1_value_topic: String,
    pm2_5_value_topic: String,
    pm10_value_topic: String,
}
