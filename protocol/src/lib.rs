#![cfg_attr(not(feature = "std"), no_std)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(Debug))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[serde(rename_all = "snake_case")]
pub enum FanSpeed {
    Low,
    Medium,
    High,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(Debug))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ExternalCommand {
    pub fan: Option<ExternalFanCommand>,
    pub speed: Option<FanSpeed>,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(Debug))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[serde(rename_all = "snake_case")]
pub enum ExternalFanCommand {
    Stop,
    RunFor { seconds: u64 },
}
