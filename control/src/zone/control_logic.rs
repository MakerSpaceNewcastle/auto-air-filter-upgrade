use crate::sensor::{
    air_quality::{AirCleanliness, AirQualitySensor},
    presence::{Presence, PresenceSensor},
    SensorRead,
};
use log::warn;
use ms_air_filter_protocol::{ExternalCommand, ExternalFanCommand, FanSpeed};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ControlMode {
    PresenceAndAirQuality,
    AirQualityOnly,
    PresenceOnly,
}

fn get_worst_air_cleanliness(
    air_quality_sensors: &[AirQualitySensor],
) -> anyhow::Result<AirCleanliness> {
    match air_quality_sensors
        .iter()
        .filter_map(|s| s.reading())
        .filter_map(|r| r.class)
        .min()
    {
        Some(cleanliness) => Ok(cleanliness),
        None => anyhow::bail!("no operational air quality sensors"),
    }
}

fn get_any_presence(presence_sensors: &[PresenceSensor]) -> anyhow::Result<bool> {
    let readings: Vec<_> = presence_sensors
        .iter()
        .filter_map(|s| s.reading())
        .collect();

    if readings.is_empty() {
        anyhow::bail!("no operational presence sensors")
    } else {
        Ok(readings.iter().any(|i| i.value == Presence::Occupied))
    }
}

fn get_fan_command_for_occupied_air_quality(cleanliness: AirCleanliness) -> ExternalCommand {
    ExternalCommand {
        fan: Some(match cleanliness {
            AirCleanliness::Clean => ExternalFanCommand::Stop,
            _ => ExternalFanCommand::RunFor { seconds: 120 },
        }),
        speed: match cleanliness {
            AirCleanliness::VeryDirty => Some(FanSpeed::High),
            AirCleanliness::Dirty => Some(FanSpeed::Medium),
            AirCleanliness::PreDirty => Some(FanSpeed::Low),
            AirCleanliness::Clean => None,
        },
    }
}

fn get_fan_command_for_presence_in_unknown_air_quality(occupied: bool) -> ExternalCommand {
    let fan = if occupied {
        ExternalFanCommand::RunFor { seconds: 120 }
    } else {
        ExternalFanCommand::Stop
    };

    ExternalCommand {
        fan: Some(fan),
        speed: None,
    }
}

pub(crate) fn stateless_process(
    mode: ControlMode,
    presence_sensors: &[PresenceSensor],
    air_quality_sensors: &[AirQualitySensor],
) -> anyhow::Result<ExternalCommand> {
    match mode {
        ControlMode::PresenceAndAirQuality => {
            let cleanliness = match get_worst_air_cleanliness(air_quality_sensors) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "Failed to get air cleanliness ({}), falling back on presence",
                        e
                    );
                    return Ok(get_fan_command_for_presence_in_unknown_air_quality(
                        get_any_presence(presence_sensors)?,
                    ));
                }
            };

            let presence = match get_any_presence(presence_sensors) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "Failed to get presence ({}), falling back on air cleanliness",
                        e
                    );
                    return Ok(get_fan_command_for_occupied_air_quality(cleanliness));
                }
            };

            if presence {
                Ok(get_fan_command_for_occupied_air_quality(cleanliness))
            } else {
                const UNOCCUPIED_RUN_THRESHOLD: AirCleanliness = AirCleanliness::VeryDirty;

                Ok(ExternalCommand {
                    fan: if cleanliness >= UNOCCUPIED_RUN_THRESHOLD {
                        Some(ExternalFanCommand::RunFor { seconds: 120 })
                    } else {
                        Some(ExternalFanCommand::Stop)
                    },
                    speed: if cleanliness >= UNOCCUPIED_RUN_THRESHOLD {
                        Some(FanSpeed::Low)
                    } else {
                        None
                    },
                })
            }
        }
        ControlMode::AirQualityOnly => {
            let cleanliness = get_worst_air_cleanliness(air_quality_sensors)?;
            Ok(get_fan_command_for_occupied_air_quality(cleanliness))
        }
        ControlMode::PresenceOnly => Ok(get_fan_command_for_presence_in_unknown_air_quality(
            get_any_presence(presence_sensors)?,
        )),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sensor::{
        air_quality::AirQualitySensorConfig, presence::PresenceSensorConfig, SensorUpdate,
    };
    use ms_air_filter_protocol::FanSpeed;
    use rumqttc::{Publish, QoS};
    use std::time::Duration;

    #[test]
    fn presence_and_air_quality_clean_and_occupied_idle() {
        let mut presence_sensors: Vec<PresenceSensor> = vec![PresenceSensorConfig {
            name: "a".into(),
            topic: "a".into(),
            timeout: 0,
        }
        .into()];

        let mut air_quality_sensors: Vec<AirQualitySensor> = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        presence_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "on"))
            .unwrap();

        air_quality_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "5"))
            .unwrap();

        let ret = stateless_process(
            ControlMode::PresenceAndAirQuality,
            &presence_sensors,
            &air_quality_sensors,
        )
        .unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::Stop),
                speed: None
            }
        );
    }

    #[test]
    fn air_quality_only_no_sensors() {
        let air_quality_sensors = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        let ret = stateless_process(
            ControlMode::AirQualityOnly,
            &Vec::new(),
            &air_quality_sensors,
        );

        assert!(ret.is_err());
    }

    #[test]
    fn air_quality_only_idle() {
        let mut air_quality_sensors: Vec<AirQualitySensor> = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        air_quality_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "5"))
            .unwrap();

        let ret = stateless_process(
            ControlMode::AirQualityOnly,
            &Vec::new(),
            &air_quality_sensors,
        )
        .unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::Stop),
                speed: None
            }
        );
    }

    #[test]
    fn air_quality_only_pre_dirty() {
        let mut air_quality_sensors: Vec<AirQualitySensor> = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        air_quality_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "9"))
            .unwrap();

        let ret = stateless_process(
            ControlMode::AirQualityOnly,
            &Vec::new(),
            &air_quality_sensors,
        )
        .unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::RunFor { seconds: 120 }),
                speed: Some(FanSpeed::Low)
            }
        );
    }

    #[test]
    fn air_quality_only_dirty() {
        let mut air_quality_sensors: Vec<AirQualitySensor> = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        air_quality_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "18.1"))
            .unwrap();

        let ret = stateless_process(
            ControlMode::AirQualityOnly,
            &Vec::new(),
            &air_quality_sensors,
        )
        .unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::RunFor { seconds: 120 }),
                speed: Some(FanSpeed::Medium)
            }
        );
    }

    #[test]
    fn air_quality_only_very_dirty() {
        let mut air_quality_sensors: Vec<AirQualitySensor> = vec![AirQualitySensorConfig {
            name: "a".into(),
            topic: "a".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        }
        .into()];

        air_quality_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "50.1"))
            .unwrap();

        let ret = stateless_process(
            ControlMode::AirQualityOnly,
            &Vec::new(),
            &air_quality_sensors,
        )
        .unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::RunFor { seconds: 120 }),
                speed: Some(FanSpeed::High)
            }
        );
    }

    #[test]
    fn presence_only_no_sensors() {
        let presence_sensors = vec![PresenceSensorConfig {
            name: "a".into(),
            topic: "a".into(),
            timeout: 20,
        }
        .into()];

        let ret = stateless_process(ControlMode::PresenceOnly, &presence_sensors, &Vec::new());

        assert!(ret.is_err());
    }

    #[test]
    fn presence_only_idle() {
        let mut presence_sensors: Vec<PresenceSensor> = vec![PresenceSensorConfig {
            name: "a".into(),
            topic: "a".into(),
            timeout: 0,
        }
        .into()];

        presence_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "on"))
            .unwrap();

        std::thread::sleep(Duration::from_secs(2));

        presence_sensors
            .first_mut()
            .unwrap()
            .update_via_time()
            .unwrap();

        let ret =
            stateless_process(ControlMode::PresenceOnly, &presence_sensors, &Vec::new()).unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::Stop),
                speed: None
            }
        );
    }

    #[test]
    fn presence_only_run() {
        let mut presence_sensors: Vec<PresenceSensor> = vec![PresenceSensorConfig {
            name: "a".into(),
            topic: "a".into(),
            timeout: 20,
        }
        .into()];

        presence_sensors
            .first_mut()
            .unwrap()
            .update_via_mqtt_message(&Publish::new("a", QoS::AtLeastOnce, "on"))
            .unwrap();

        let ret =
            stateless_process(ControlMode::PresenceOnly, &presence_sensors, &Vec::new()).unwrap();

        assert_eq!(
            ret,
            ExternalCommand {
                fan: Some(ExternalFanCommand::RunFor { seconds: 120 }),
                speed: None
            }
        );
    }
}
