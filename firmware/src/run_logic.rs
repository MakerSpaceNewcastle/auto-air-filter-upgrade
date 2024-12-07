use crate::{
    fan::{FanCommand, FanSpeed, FAN_SPEED},
    maybe_timer::MaybeTimer,
    presence_sensors::{Presence, PRESENCE_EVENTS},
    ui_buttons::{UiEvent, UI_EVENTS},
};
use defmt::{info, warn, Format};
use embassy_futures::select::{select3, Either3};
use embassy_sync::pubsub::WaitResult;
use embassy_time::{Duration, Instant};

#[derive(Clone, Eq, PartialEq, Format)]
enum FanRunning {
    Stopped,
    Running { until: Option<Instant> },
}

impl FanRunning {
    fn run_for(d: Duration) -> Self {
        Self::Running {
            until: Some(Instant::now() + d),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Format)]
struct State {
    fan: FanRunning,
    fan_speed: FanSpeed,
    presence: Presence,
}

impl State {
    fn get_fan_command(&self) -> FanCommand {
        match self.fan {
            FanRunning::Stopped => FanCommand::Stop,
            FanRunning::Running { .. } => FanCommand::Run(self.fan_speed.clone()),
        }
    }
}

const TIMEOUT: Duration = Duration::from_secs(60 * 5); // 5 minutes

#[embassy_executor::task]
pub(crate) async fn task() {
    let mut presence_rx = PRESENCE_EVENTS.subscriber().unwrap();
    let mut ui_rx = UI_EVENTS.subscriber().unwrap();

    let mut state = State {
        fan: FanRunning::Stopped,
        fan_speed: FanSpeed::Low,
        presence: Presence::Clear,
    };

    let fan_tx = FAN_SPEED.publisher().unwrap();

    loop {
        let fan_off_time = match state.fan {
            FanRunning::Stopped => None,
            FanRunning::Running { until } => until,
        };

        match select3(
            presence_rx.next_message(),
            ui_rx.next_message(),
            MaybeTimer::at(fan_off_time),
        )
        .await
        {
            Either3::First(msg) => match msg {
                WaitResult::Lagged(msg_count) => {
                    warn!(
                        "Lagged listening to presence events, missed {} messages",
                        msg_count
                    )
                }
                WaitResult::Message(msg) => {
                    state.presence = msg.state;

                    if state.presence == Presence::Occupied {
                        state.fan = FanRunning::run_for(TIMEOUT);
                    }
                }
            },
            Either3::Second(msg) => match msg {
                WaitResult::Lagged(msg_count) => {
                    warn!(
                        "Lagged listening to UI events, missed {} messages",
                        msg_count
                    )
                }
                WaitResult::Message(UiEvent::SpeedButtonPushed) => {
                    state.fan_speed = match state.fan {
                        FanRunning::Running { .. } => {
                            let mut speed = state.fan_speed.clone();
                            speed.cycle();
                            speed
                        }
                        FanRunning::Stopped => FanSpeed::Low,
                    };

                    state.fan = FanRunning::run_for(TIMEOUT);
                }
            },
            Either3::Third(_) => {
                info!("Turning off fan after timeout");
                state.fan = FanRunning::Stopped;
            }
        };

        info!("State: {}", state);
        fan_tx.publish_immediate(state.get_fan_command());
    }
}
