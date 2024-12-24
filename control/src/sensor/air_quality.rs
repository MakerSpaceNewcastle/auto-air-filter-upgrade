use super::{SensorRead, SensorReading, SensorUpdate};
use crate::Update;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct AirQualitySensorConfig {
    pub(crate) name: String,
    pub(crate) topic: String,

    pub(crate) pre_dirty_threshold: f64,
    pub(crate) dirty_threshold: f64,
    pub(crate) very_dirty_threshold: f64,
}

impl AirQualitySensorConfig {
    fn classify_reading(&self, value: f64) -> AirCleanliness {
        if value >= self.very_dirty_threshold {
            AirCleanliness::VeryDirty
        } else if value >= self.dirty_threshold {
            AirCleanliness::Dirty
        } else if value >= self.pre_dirty_threshold {
            AirCleanliness::PreDirty
        } else {
            AirCleanliness::Clean
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AirCleanliness {
    VeryDirty,
    Dirty,
    PreDirty,
    Clean,
}

pub(crate) struct AirQualitySensor {
    config: AirQualitySensorConfig,
    reading: Option<SensorReading<f64, AirCleanliness>>,
}

impl From<AirQualitySensorConfig> for AirQualitySensor {
    fn from(config: AirQualitySensorConfig) -> Self {
        Self {
            config,
            reading: None,
        }
    }
}

impl SensorRead<f64, AirCleanliness> for AirQualitySensor {
    fn name(&self) -> &str {
        self.config.name.as_str()
    }

    fn reading(&self) -> Option<SensorReading<f64, AirCleanliness>> {
        self.reading.clone()
    }
}

impl SensorUpdate for AirQualitySensor {
    fn update_via_mqtt_message(&mut self, msg: &rumqttc::Publish) -> anyhow::Result<Update> {
        if self.config.topic == msg.topic {
            let s = std::str::from_utf8(&msg.payload)?;

            let value: f64 = s.parse()?;

            self.reading = Some(SensorReading::new(
                value,
                Some(self.config.classify_reading(value)),
            ));

            Ok(Update::Updated)
        } else {
            Ok(Update::NotUpdated)
        }
    }

    fn update_via_time(&mut self) -> anyhow::Result<Update> {
        Ok(Update::NotUpdated)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rumqttc::{Publish, QoS};

    #[test]
    fn cleanliness_order() {
        assert!(AirCleanliness::Clean > AirCleanliness::PreDirty);

        let v = vec![AirCleanliness::PreDirty, AirCleanliness::VeryDirty];
        assert_eq!(v.into_iter().min().unwrap(), AirCleanliness::VeryDirty);
    }

    #[test]
    fn basic() {
        let config = AirQualitySensorConfig {
            name: "test sensor".into(),
            topic: "test/value".into(),
            pre_dirty_threshold: 8.0,
            dirty_threshold: 15.0,
            very_dirty_threshold: 50.0,
        };

        let mut sensor: AirQualitySensor = config.clone().into();
        assert_eq!(sensor.config, config);

        assert_eq!(sensor.reading(), None);

        assert_eq!(
            sensor
                .update_via_mqtt_message(&Publish::new("test/value", QoS::ExactlyOnce, b"2.0"))
                .ok(),
            Some(Update::Updated)
        );

        let reading = sensor.reading().unwrap();
        assert_eq!(reading.value, 2.0);
        assert_eq!(reading.class, Some(AirCleanliness::Clean));

        assert_eq!(
            sensor
                .update_via_mqtt_message(&Publish::new("test/value", QoS::ExactlyOnce, b"9.0"))
                .ok(),
            Some(Update::Updated)
        );

        let reading = sensor.reading().unwrap();
        assert_eq!(reading.value, 9.0);
        assert_eq!(reading.class, Some(AirCleanliness::PreDirty));

        assert_eq!(
            sensor
                .update_via_mqtt_message(&Publish::new("test/value", QoS::ExactlyOnce, b"20.0"))
                .ok(),
            Some(Update::Updated)
        );

        let reading = sensor.reading().unwrap();
        assert_eq!(reading.value, 20.0);
        assert_eq!(reading.class, Some(AirCleanliness::Dirty));

        assert_eq!(
            sensor
                .update_via_mqtt_message(&Publish::new("test/value", QoS::ExactlyOnce, b"55.0"))
                .ok(),
            Some(Update::Updated)
        );

        let reading = sensor.reading().unwrap();
        assert_eq!(reading.value, 55.0);
        assert_eq!(reading.class, Some(AirCleanliness::VeryDirty));

        assert_eq!(
            sensor
                .update_via_mqtt_message(&Publish::new("test/value", QoS::ExactlyOnce, b"55.0"))
                .ok(),
            Some(Update::Updated)
        );

        let reading2 = sensor.reading().unwrap();
        assert_eq!(reading2.value, 55.0);
        assert_eq!(reading2.class, Some(AirCleanliness::VeryDirty));

        assert_ne!(reading, reading2);
    }
}
