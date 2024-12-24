use crate::{
    fan::{FanCommand, FAN_SPEED},
    maybe_timer::MaybeTimer,
};
use defmt::{info, warn, Format};
use embassy_futures::select::{select, Either};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{PubSubChannel, WaitResult},
};
use embassy_time::{Duration, Instant};
use ms_air_filter_protocol::{ExternalCommand, ExternalFanCommand, FanSpeed};

pub(crate) static EXTERNAL_COMMAND: PubSubChannel<
    CriticalSectionRawMutex,
    ExternalCommand,
    1,
    1,
    2,
> = PubSubChannel::new();

#[derive(Clone, Eq, PartialEq, Format)]
struct State {
    fan: FanRunning,
    speed: FanSpeed,
}

impl State {
    fn get_fan_command(&self) -> FanCommand {
        match self.fan {
            FanRunning::Stopped => FanCommand::Stop,
            FanRunning::Running { .. } => FanCommand::Run(self.speed.clone()),
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
        speed: FanSpeed::Low,
    };

    let mut command_sub = EXTERNAL_COMMAND.subscriber().unwrap();
    let fan_pub = FAN_SPEED.publisher().unwrap();

    loop {
        let fan_off_time = match state.fan {
            FanRunning::Stopped => None,
            FanRunning::Running { until } => Some(until),
        };

        let new_state = match select(command_sub.next_message(), MaybeTimer::at(fan_off_time)).await
        {
            Either::First(WaitResult::Lagged(lost)) => {
                warn!("Command subscriber lagged, lost {} messages", lost);
                None
            }
            Either::First(WaitResult::Message(cmd)) => Some(State {
                fan: match cmd.fan {
                    Some(fan) => fan.into(),
                    None => state.fan.clone(),
                },
                speed: match cmd.speed {
                    Some(speed) => speed,
                    None => state.speed.clone(),
                },
            }),
            Either::Second(_) => {
                info!("Turning off fan after timeout");
                Some(State {
                    fan: FanRunning::Stopped,
                    speed: state.speed.clone(),
                })
            }
        };

        if let Some(new_state) = new_state {
            if new_state != state {
                state = new_state;
                info!("State: {}", state);
                fan_pub.publish_immediate(state.get_fan_command());
            }
        }
    }
}
