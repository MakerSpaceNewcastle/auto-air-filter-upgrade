use crate::{
    fan::{FanCommand, FanSpeed, FAN_SPEED},
    maybe_timer::MaybeTimer,
};
use defmt::{info, Format};
use embassy_futures::select::{select, Either};
use embassy_time::Instant;

#[derive(Clone, Eq, PartialEq, Format)]
struct ExternalCommand {
    fan: Option<ExternalFanCommand>,
    speed: Option<FanSpeed>,
}

#[derive(Clone, Eq, PartialEq, Format)]
enum ExternalFanCommand {
    Stop,
    RunFor { seconds: u32 },
}

#[derive(Clone, Eq, PartialEq, Format)]
struct State {
    fan: FanRunning,
    fan_speed: FanSpeed,
}

impl State {
    fn get_fan_command(&self) -> FanCommand {
        match self.fan {
            FanRunning::Stopped => FanCommand::Stop,
            FanRunning::Running { .. } => FanCommand::Run(self.fan_speed.clone()),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Format)]
enum FanRunning {
    Stopped,
    Running { until: Option<Instant> },
}

#[embassy_executor::task]
pub(crate) async fn task() {
    let mut state = State {
        fan: FanRunning::Stopped,
        fan_speed: FanSpeed::Low,
    };

    let fan_tx = FAN_SPEED.publisher().unwrap();

    loop {
        let fan_off_time = match state.fan {
            FanRunning::Stopped => None,
            FanRunning::Running { until } => until,
        };

        match select(MaybeTimer::at(None), MaybeTimer::at(fan_off_time)).await {
            Either::First(_) => {
                // TODO
            }
            Either::Second(_) => {
                info!("Turning off fan after timeout");
                state.fan = FanRunning::Stopped;
            }
        };

        info!("State: {}", state);
        fan_tx.publish_immediate(state.get_fan_command());
    }
}
