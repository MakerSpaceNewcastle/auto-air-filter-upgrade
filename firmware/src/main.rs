#![no_std]
#![no_main]

mod fan;
mod maybe_timer;
mod run_logic;
mod temperature_sensors;
mod wifi;

use defmt::{info, unwrap};
use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals,
    watchdog::Watchdog,
};
use embassy_time::{Duration, Timer};
use ms_air_filter_protocol::{ExternalCommand, ExternalFanCommand};
#[cfg(feature = "panic-probe")]
use panic_probe as _;
use run_logic::EXTERNAL_COMMAND;
use static_cell::StaticCell;

#[cfg(not(feature = "panic-probe"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    use embassy_rp::gpio::{Level, Output};

    let p = unsafe { embassy_rp::Peripherals::steal() };
    let r = split_resources!(p);

    // Turn off all fan output contactors
    let r = r.fan_relays;
    let _fan_high = Output::new(r.high, Level::Low);
    let _fan_medium = Output::new(r.medium, Level::Low);
    let _fan_low = Output::new(r.low, Level::Low);
    let _contactor_voltage = Output::new(r.contactor_voltage, Level::Low);

    loop {
        embassy_time::block_for(Duration::from_secs(10));
    }
}

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

assign_resources::assign_resources! {
    fan_relays: FanRelayResources {
        low: PIN_16,
        medium: PIN_6,
        high: PIN_7,
        contactor_voltage: PIN_17,
    },
    onewire: OnewireResources {
        data: PIN_22,
    },
    status: StatusResources {
        watchdog: WATCHDOG,
    },
    wifi: WifiResources {
        pwr: PIN_23,
        cs: PIN_25,
        pio: PIO0,
        dio: PIN_24,
        clk: PIN_29,
        dma_ch: DMA_CH0,
    },
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);

    info!("Version: {}", env!("VERSION"));

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                unwrap!(spawner.spawn(watchdog_feed(r.status)));

                unwrap!(spawner.spawn(crate::fan::task(r.fan_relays)));

                unwrap!(spawner.spawn(crate::run_logic::task()));
            });
        },
    );

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        unwrap!(spawner.spawn(crate::temperature_sensors::task(r.onewire)));

        unwrap!(spawner.spawn(crate::wifi::task(r.wifi, spawner)));

        unwrap!(spawner.spawn(initial_fan_trigger_task()));
    });
}

#[embassy_executor::task]
async fn watchdog_feed(r: StatusResources) {
    let mut watchdog = Watchdog::new(r.watchdog);
    watchdog.start(Duration::from_millis(550));

    loop {
        watchdog.feed();
        Timer::after_millis(500).await;
    }
}

#[embassy_executor::task]
async fn initial_fan_trigger_task() {
    let cmd_pub = EXTERNAL_COMMAND.publisher().unwrap();

    Timer::after_secs(5).await;

    info!("Triggering initial fan run after power on");
    cmd_pub
        .publish(ExternalCommand {
            fan: Some(ExternalFanCommand::RunFor { seconds: 300 }),
            speed: None,
        })
        .await;
}
