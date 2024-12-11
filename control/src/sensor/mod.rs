pub(crate) mod air_quality;
pub(crate) mod presence;

use crate::Update;
use rumqttc::Publish;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SensorReading<V, C> {
    pub value: V,
    pub class: Option<C>,
    pub at: Instant,
}

impl<V, C> SensorReading<V, C> {
    pub(crate) fn new(value: V, class: Option<C>) -> Self {
        Self {
            value,
            class,
            at: Instant::now(),
        }
    }

    fn age(&self) -> Duration {
        Instant::now() - self.at
    }
}

pub(crate) trait SensorRead<V, C> {
    fn name(&self) -> &str;
    fn reading(&self) -> Option<SensorReading<V, C>>;
}

pub(crate) trait SensorUpdate {
    fn update_via_mqtt_message(&mut self, msg: &Publish) -> anyhow::Result<Update>;
    fn update_via_time(&mut self) -> anyhow::Result<Update>;
}
