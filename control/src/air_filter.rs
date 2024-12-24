use ms_air_filter_protocol::ExternalCommand;
use rumqttc::AsyncClient;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct AirFilterConfig {
    #[allow(unused)]
    name: String,
    command_topic: String,
}

pub(crate) struct AirFilter {
    mqtt_client: AsyncClient,
    config: AirFilterConfig,
}

impl AirFilter {
    pub(crate) fn new(mqtt_client: AsyncClient, config: AirFilterConfig) -> Self {
        Self {
            mqtt_client,
            config,
        }
    }

    pub(crate) async fn command(&self, command: ExternalCommand) -> anyhow::Result<()> {
        let s = serde_json::to_string(&command)?;

        self.mqtt_client
            .publish(
                &self.config.command_topic,
                rumqttc::QoS::AtLeastOnce,
                false,
                s,
            )
            .await?;

        Ok(())
    }
}
