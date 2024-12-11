use crate::zone::ZoneConfig;
use rand::{distributions::Alphanumeric, Rng};
use rumqttc::MqttOptions;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub mqtt: MqttConfig,
    pub zones: Vec<ZoneConfig>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MqttConfig {
    host: String,
    port: u16,

    username: String,
    password: String,
}

fn generate_client_id() -> String {
    let r: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect();
    format!("ms-air-filter-control-{r}")
}

impl From<MqttConfig> for MqttOptions {
    fn from(value: MqttConfig) -> Self {
        let mut options = Self::new(generate_client_id(), value.host, value.port);
        options.set_credentials(value.username, value.password);
        options.set_keep_alive(Duration::from_secs(5));
        options
    }
}
