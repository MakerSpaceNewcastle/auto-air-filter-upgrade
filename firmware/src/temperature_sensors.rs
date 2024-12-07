use defmt::{debug, info, Format};
use ds18b20::{Ds18b20, Resolution};
use embassy_rp::gpio::{Level, OutputOpenDrain};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::PubSubChannel};
use embassy_time::{Delay, Duration, Ticker, Timer};
use one_wire_bus::{Address, OneWire};

pub(crate) type TemperatureReading = Result<f32, ()>;

#[derive(Clone, Format)]
pub(crate) struct Temperatures {
    pub(crate) onboard: TemperatureReading,
}

pub(crate) static TEMPERATURE_READING: PubSubChannel<
    CriticalSectionRawMutex,
    Temperatures,
    8,
    1,
    1,
> = PubSubChannel::new();

#[embassy_executor::task]
pub(super) async fn task(r: crate::OnewireResources) {
    let mut bus = {
        let pin = OutputOpenDrain::new(r.data, Level::Low);
        OneWire::new(pin).unwrap()
    };

    for device_address in bus.devices(false, &mut Delay) {
        let device_address = device_address.unwrap();
        info!("Found one wire device at address: {}", device_address.0);
    }

    info!(
        "Configured board temperature sensor address: {}",
        env!("BOARD_TEMP_SENSOR_ADDRESS")
    );
    let onboard_temp_sensor =
        Ds18b20::new::<()>(Address(env!("BOARD_TEMP_SENSOR_ADDRESS").parse().unwrap())).unwrap();

    let mut ticker = Ticker::every(Duration::from_secs(5));
    let publisher = TEMPERATURE_READING.publisher().unwrap();

    loop {
        ticker.next().await;

        ds18b20::start_simultaneous_temp_measurement(&mut bus, &mut Delay).unwrap();

        Timer::after_millis(Resolution::Bits12.max_measurement_time_millis() as u64).await;

        let readings = Temperatures {
            onboard: onboard_temp_sensor
                .read_data(&mut bus, &mut Delay)
                .map(|v| v.temperature)
                .map_err(|_| ()),
        };

        debug!("Temperature readings: {}", readings);
        publisher.publish(readings).await;
    }
}
