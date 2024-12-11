use crate::{
    fan::FAN_SPEED, run_logic::EXTERNAL_COMMAND, temperature_sensors::TEMPERATURE_READING,
};
use cyw43::{PowerManagementMode, State};
use cyw43_pio::PioSpi;
use defmt::{debug, info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_futures::select::{select4, Either4};
use embassy_net::{tcp::TcpSocket, Config, IpAddress, Ipv4Address, Stack, StackResources};
use embassy_rp::{
    bind_interrupts,
    clocks::RoscRng,
    gpio::{Level, Output},
    peripherals::{DMA_CH0, PIO0},
    pio::{InterruptHandler, Pio},
};
use embassy_sync::pubsub::WaitResult;
use embassy_time::{Duration, Ticker, Timer};
use ms_air_filter_protocol::ExternalCommand;
use rand::RngCore;
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    packet::v5::{publish_packet::QualityOfService, reason_codes::ReasonCode},
    utils::rng_generator::CountingRng,
};
use static_cell::StaticCell;

const WIFI_SSID: &str = "Maker Space";

const MQTT_BROKER_IP: IpAddress = IpAddress::Ipv4(Ipv4Address::new(192, 168, 8, 183));
const MQTT_BROKER_PORT: u16 = 1883;

const MQTT_USERNAME: &str = "airfilter";

const MQTT_BUFFER_SIZE: usize = 512;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
pub(super) async fn task(r: crate::WifiResources, spawner: Spawner) {
    let pwr = Output::new(r.pwr, Level::Low);
    let cs = Output::new(r.cs, Level::High);

    let mut pio = Pio::new(r.pio, Irqs);

    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        cs,
        r.dio,
        r.clk,
        r.dma_ch,
    );

    static STATE: StaticCell<State> = StaticCell::new();
    let state = STATE.init(State::new());

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");
    control.init(clm).await;

    control
        .set_power_management(PowerManagementMode::None)
        .await;

    let mut rng = RoscRng;
    let seed = rng.next_u64();

    static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    unwrap!(spawner.spawn(net_task(runner)));

    info!("Joining WiFi network {}", WIFI_SSID);
    loop {
        match control.join_wpa2(WIFI_SSID, env!("WIFI_PASSWORD")).await {
            Ok(_) => break,
            Err(err) => {
                warn!("Failed to join WiFi network with status {}", err.status);
            }
        }
    }

    // Get configuration via DHCP
    info!("Waiting for DHCP");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up");

    loop {
        // Start the MQTT client
        let _ = run_mqtt_client(stack).await;

        // Wait a little bit of time before connecting again
        Timer::after_millis(500).await;
    }
}

trait ClientExt {
    async fn publish<'a>(
        &mut self,
        topic: &'a str,
        payload: &'a [u8],
        retain: bool,
    ) -> Result<(), ()>;
}

impl<T: embedded_io_async::Read + embedded_io_async::Write, R: RngCore> ClientExt
    for MqttClient<'_, T, 5, R>
{
    async fn publish<'a>(
        &mut self,
        topic: &'a str,
        payload: &'a [u8],
        retain: bool,
    ) -> Result<(), ()> {
        let result = self
            .send_message(topic, payload, QualityOfService::QoS1, retain)
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(ReasonCode::NoMatchingSubscribers) => Ok(()),
            Err(e) => {
                warn!("MQTT publish error: {:?}", e);
                Err(())
            }
        }
    }
}

async fn run_mqtt_client(stack: Stack<'_>) -> Result<(), ()> {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    let mut mqtt_rx_buffer = [0; MQTT_BUFFER_SIZE];
    let mut mqtt_tx_buffer = [0; MQTT_BUFFER_SIZE];

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    info!(
        "Connecting to MQTT broker {}:{}",
        MQTT_BROKER_IP, MQTT_BROKER_PORT
    );
    socket
        .connect((MQTT_BROKER_IP, MQTT_BROKER_PORT))
        .await
        .map_err(|e| {
            warn!("Broker socket connection error: {:?}", e);
        })?;

    let mut client = {
        let mut config = ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20000));
        config.add_client_id(env!("MQTT_CLIENT_ID"));
        config.add_username(MQTT_USERNAME);
        config.add_password(env!("MQTT_PASSWORD"));
        config.max_packet_size = MQTT_BUFFER_SIZE as u32;
        config.add_will(env!("ONLINE_MQTT_TOPIC"), b"false", true);

        MqttClient::<_, 5, _>::new(
            socket,
            &mut mqtt_tx_buffer,
            MQTT_BUFFER_SIZE,
            &mut mqtt_rx_buffer,
            MQTT_BUFFER_SIZE,
            config,
        )
    };

    match client.connect_to_broker().await {
        Ok(()) => {
            info!("Connected to MQTT broker");
        }
        Err(e) => {
            warn!("Connect: MQTT error: {:?}", e);
            return Err(());
        }
    }

    client
        .subscribe_to_topic(env!("FAN_COMMAND_TOPIC"))
        .await
        .map_err(|e| warn!("Subscribe: MQTT error: {:?}", e))?;

    client
        .publish(env!("ONLINE_MQTT_TOPIC"), b"true", true)
        .await?;

    client
        .publish(env!("VERSION_MQTT_TOPIC"), env!("VERSION").as_bytes(), true)
        .await?;

    let mut ping_tick = Ticker::every(Duration::from_secs(5));
    let mut temperature_sub = TEMPERATURE_READING.subscriber().unwrap();
    let mut fan_sub = FAN_SPEED.subscriber().unwrap();
    let cmd_pub = EXTERNAL_COMMAND.publisher().unwrap();

    loop {
        match select4(
            ping_tick.next(),
            temperature_sub.next_message(),
            fan_sub.next_message(),
            client.receive_message_if_ready(),
        )
        .await
        {
            Either4::First(_) => match client.send_ping().await {
                Ok(()) => {
                    debug!("MQTT ping OK");
                }
                Err(e) => {
                    warn!("Ping: MQTT error: {:?}", e);
                    return Err(());
                }
            },
            Either4::Second(temperatures) => match temperatures {
                WaitResult::Lagged(lost) => {
                    warn!("Temperature subscriber lagged, lost {} messages", lost);
                }
                WaitResult::Message(temperatures) => {
                    match serde_json_core::to_vec::<_, 16>(&temperatures.onboard.ok()) {
                        Ok(data) => {
                            client
                                .publish(env!("ONBOARD_TEMPERATURE_SENSOR_TOPIC"), &data, false)
                                .await?;
                        }
                        Err(e) => warn!("Failed to serialize message: {}", e),
                    }
                }
            },
            Either4::Third(fan) => match fan {
                WaitResult::Lagged(lost) => {
                    warn!("Fan subscriber lagged, lost {} messages", lost);
                }
                WaitResult::Message(fan) => {
                    let fan: &str = fan.into();
                    client
                        .publish(env!("FAN_TOPIC"), fan.as_bytes(), false)
                        .await?;
                }
            },
            Either4::Fourth(msg) => match msg {
                Ok(None) => Timer::after_millis(10).await,
                Ok(Some((topic, msg))) => {
                    debug!(
                        "Got MQTT message on topic {} with length {}",
                        topic,
                        msg.len()
                    );
                    if topic == env!("FAN_COMMAND_TOPIC") {
                        match serde_json_core::from_slice::<ExternalCommand>(msg) {
                            Ok(cmd) => {
                                info!("External command via MQTT: {}", cmd);
                                cmd_pub.publish(cmd.0).await;
                            }
                            Err(e) => warn!("Failed to parse command message: {}", e),
                        }
                    }
                }
                Err(rc) => {
                    warn!("MQTT receive error, rc={}", rc);
                    return Err(());
                }
            },
        }
    }
}
