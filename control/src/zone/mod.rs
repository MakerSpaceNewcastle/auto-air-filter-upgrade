mod control_logic;

use self::control_logic::ControlMode;
use crate::{
    air_filter::{AirFilter, AirFilterConfig},
    sensor::{
        air_quality::{AirQualitySensor, AirQualitySensorConfig},
        presence::{PresenceSensor, PresenceSensorConfig},
        SensorRead, SensorUpdate,
    },
    Update,
};
use log::{info, warn};
use rumqttc::{AsyncClient, Publish};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ZoneConfig {
    name: String,

    filter: AirFilterConfig,

    #[serde(default)]
    presence: Vec<PresenceSensorConfig>,

    #[serde(default)]
    air_quality: Vec<AirQualitySensorConfig>,
}

pub(crate) struct Zone {
    name: String,

    filter: AirFilter,

    presence_sensors: Vec<PresenceSensor>,
    air_quality_sensors: Vec<AirQualitySensor>,
}

impl Zone {
    pub(crate) fn new(mqtt_client: AsyncClient, config: ZoneConfig) -> Self {
        let presence_sensors = config.presence.into_iter().map(|c| c.into()).collect();
        let air_quality_sensors = config.air_quality.into_iter().map(|c| c.into()).collect();

        Self {
            name: config.name,
            filter: AirFilter::new(mqtt_client, config.filter),
            presence_sensors,
            air_quality_sensors,
        }
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    fn mode(&self) -> ControlMode {
        let have_presence = !self.presence_sensors.is_empty();
        let have_air_quality = !self.air_quality_sensors.is_empty();

        if have_presence && have_air_quality {
            ControlMode::PresenceAndAirQuality
        } else if have_presence {
            ControlMode::PresenceOnly
        } else if have_air_quality {
            ControlMode::AirQualityOnly
        } else {
            unreachable!()
        }
    }

    pub(crate) async fn evaluate_and_send_command(&self) -> anyhow::Result<()> {
        let command = self::control_logic::stateless_process(
            self.mode(),
            &self.presence_sensors,
            &self.air_quality_sensors,
        )?;
        info!("Air filter command for zone {}: {:?}", self.name, command);

        self.filter.command(command).await?;

        Ok(())
    }
}

impl SensorUpdate for Zone {
    fn update_via_mqtt_message(&mut self, msg: &Publish) -> anyhow::Result<Update> {
        let mut res = Update::NotUpdated;

        update_via_mqtt_message_helper(&mut res, msg, &mut self.presence_sensors);
        update_via_mqtt_message_helper(&mut res, msg, &mut self.air_quality_sensors);

        Ok(res)
    }

    fn update_via_time(&mut self) -> anyhow::Result<Update> {
        let mut res = Update::NotUpdated;

        update_via_time_helper(&mut res, &mut self.presence_sensors);
        update_via_time_helper(&mut res, &mut self.air_quality_sensors);

        Ok(res)
    }
}

fn update_via_mqtt_message_helper<T: SensorRead<V, C> + SensorUpdate, V, C>(
    res: &mut Update,
    msg: &Publish,
    sensors: &mut [T],
) {
    for s in sensors.iter_mut() {
        match s.update_via_mqtt_message(msg) {
            Ok(Update::Updated) => {
                *res = Update::Updated;
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to update sensor {} because {e}", s.name()),
        }
    }
}

fn update_via_time_helper<T: SensorRead<V, C> + SensorUpdate, V, C>(
    res: &mut Update,
    sensors: &mut [T],
) {
    for s in sensors.iter_mut() {
        match s.update_via_time() {
            Ok(Update::Updated) => {
                *res = Update::Updated;
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to update sensor {} because {e}", s.name()),
        }
    }
}
