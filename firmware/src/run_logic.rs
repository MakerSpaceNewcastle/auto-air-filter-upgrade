use crate::{
    fan::{FanCommand, FanSpeed, FAN_SPEED},
    maybe_timer::MaybeTimer,
};
use defmt::{info, warn, Format};
use embassy_futures::select::{select, Either};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{PubSubChannel, WaitResult},
};
use embassy_time::{Duration, Instant};

#[derive(Clone, Eq, PartialEq, Format)]
pub(crate) struct ExternalCommand {
    fan: Option<ExternalFanCommand>,
    speed: Option<FanSpeed>,
}

#[derive(Clone, Eq, PartialEq, Format)]
pub(crate) enum ExternalFanCommand {
    Stop,
    RunFor { seconds: u64 },
}

pub(crate) static EXTERNAL_COMMAND: PubSubChannel<
    CriticalSectionRawMutex,
    ExternalCommand,
    1,
    1,
    1,
> = PubSubChannel::new();

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
    Running { until: Instant },
}

impl From<ExternalFanCommand> for FanRunning {
    fn from(value: ExternalFanCommand) -> Self {
        match value {
            ExternalFanCommand::Stop => Self::Stopped,
            ExternalFanCommand::RunFor { seconds } => Self::Running {
                until: Instant::now() + Duration::from_secs(seconds),
            },
        }
    }
}

#[embassy_executor::task]
pub(crate) async fn task() {
    let mut state = State {
        fan: FanRunning::Stopped,
        fan_speed: FanSpeed::Low,
    };

    let mut command_sub = EXTERNAL_COMMAND.subscriber().unwrap();
    let fan_pub = FAN_SPEED.publisher().unwrap();

    loop {
        let fan_off_time = match state.fan {
            FanRunning::Stopped => None,
            FanRunning::Running { until } => Some(until),
        };

        match select(command_sub.next_message(), MaybeTimer::at(fan_off_time)).await {
            Either::First(WaitResult::Lagged(lost)) => {
                warn!("Command subscriber lagged, lost {} messages", lost);
            }
            Either::First(WaitResult::Message(cmd)) => {
                // TODO
            }
            Either::Second(_) => {
                info!("Turning off fan after timeout");
                state.fan = FanRunning::Stopped;
            }
        };

        info!("State: {}", state);
        fan_pub.publish_immediate(state.get_fan_command());
    }
}
