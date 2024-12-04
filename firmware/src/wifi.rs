use cyw43::{PowerManagementMode, State};
use cyw43_pio::PioSpi;
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, Config, IpAddress, Ipv4Address, Stack, StackResources};
use embassy_rp::{
    bind_interrupts,
    clocks::RoscRng,
    gpio::{Level, Output},
    peripherals::{DMA_CH0, PIO0},
    pio::{InterruptHandler, Pio},
};
use embassy_time::{Duration, Timer};
use rand::RngCore;
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    packet::v5::publish_packet::QualityOfService,
    utils::rng_generator::CountingRng,
};
use static_cell::StaticCell;

const WIFI_SSID: &str = "Maker Space";

pub(crate) const MQTT_BROKER_IP: IpAddress = IpAddress::Ipv4(Ipv4Address::new(192, 168, 8, 183));
const MQTT_BROKER_PORT: u16 = 1883;

const MQTT_CLIENT_ID: &str = "hoshiguma-telemetry-module";
const MQTT_USERNAME: &str = "hoshiguma";

const ONLINE_MQTT_TOPIC: &str = "hoshiguma/telemetry-module/online";
const VERSION_MQTT_TOPIC: &str = "hoshiguma/telemetry-module/version";
const TELEMETRY_MQTT_TOPIC: &str = "hoshiguma/events";

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

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
        .set_power_management(PowerManagementMode::PowerSave)
        .await;

    let mut rng = RoscRng;
    let seed = rng.next_u64();

    static STACK: StaticCell<Stack<cyw43::NetDriver<'static>>> = StaticCell::new();
    static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let stack = &*STACK.init(Stack::new(
        net_device,
        Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::<4>::new()),
        seed,
    ));

    unwrap!(spawner.spawn(net_task(stack)));

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    const MQTT_BUFFER_SIZE: usize = 512;
    let mut mqtt_rx_buffer = [0; MQTT_BUFFER_SIZE];
    let mut mqtt_tx_buffer = [0; MQTT_BUFFER_SIZE];

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
    {
        info!("Waiting for DHCP");
        while !stack.is_config_up() {
            Timer::after_millis(100).await;
        }
        info!("DHCP is now up");

        let config = stack.config_v4().unwrap();
    }

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        info!(
            "Connecting to MQTT broker {}:{}",
            MQTT_BROKER_IP, MQTT_BROKER_PORT
        );
        let connection = socket.connect((MQTT_BROKER_IP, MQTT_BROKER_PORT)).await;
        if let Err(e) = connection {
            warn!("Broker socket connection error: {:?}", e);
            continue;
        }

        let mut client = {
            let mut config = ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20000));
            config.add_client_id(MQTT_CLIENT_ID);
            config.add_username(MQTT_USERNAME);
            config.add_password(env!("MQTT_PASSWORD"));
            config.max_packet_size = MQTT_BUFFER_SIZE as u32;
            config.add_will(ONLINE_MQTT_TOPIC, b"false", true);

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
                warn!("MQTT error: {:?}", e);
                continue;
            }
        }

        match client
            .send_message(ONLINE_MQTT_TOPIC, b"true", QualityOfService::QoS1, true)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                warn!("MQTT error: {:?}", e);
                continue;
            }
        }

        match client
            .send_message(
                VERSION_MQTT_TOPIC,
                git_version::git_version!().as_bytes(),
                QualityOfService::QoS1,
                true,
            )
            .await
        {
            Ok(()) => {}
            Err(e) => {
                warn!("MQTT error: {:?}", e);
                continue;
            }
        }

        loop {
            // TODO
            embassy_time::Timer::after_secs(10).await;
        }
    }
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}
