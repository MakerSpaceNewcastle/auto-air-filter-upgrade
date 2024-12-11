mod air_filter;
mod app_config;
mod sensor;
mod zone;

use clap::Parser;
use log::{debug, info, trace, warn};
use rumqttc::{AsyncClient, Event, Packet, Publish};
use sensor::SensorUpdate;
use std::time::Duration;
use zone::Zone;

#[derive(Debug, Parser)]
#[command(version = env!("VERSION"), about)]
struct Cli {
    /// Configuration file
    #[arg(short, long)]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    env_logger::init();

    let config = config::Config::builder()
        .add_source(config::File::with_name(&args.config))
        .build()
        .unwrap();
    let config = config.try_deserialize::<app_config::Config>().unwrap();

    let (mqtt_client, mut mqtt_connection) = AsyncClient::new(config.mqtt.into(), 16);

    let mut zones: Vec<Zone> = config
        .zones
        .into_iter()
        .map(|config| Zone::new(mqtt_client.clone(), config))
        .collect();

    let mut sensor_update_interval = tokio::time::interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Exiting");
                break;
            }
            event = mqtt_connection.poll() => {
                trace!("MQTT event: {:?}", event);
                match event {
                    Ok(Event::Incoming(Packet::Publish(msg))) => update_zones_via_mqtt_message(&mut zones, &msg).await,
                    Err(e) => warn!("MQTT error: {:?}", e),
                    _ => {}
                }
            }
            _ = sensor_update_interval.tick() => update_zones_via_time(&mut zones).await,
        };
    }
}

async fn update_zones_via_mqtt_message(zones: &mut [Zone], msg: &Publish) {
    for zone in zones.iter_mut() {
        match zone.update_via_mqtt_message(msg) {
            Ok(Update::Updated) => {
                if let Err(e) = zone.evaluate_and_send_command().await {
                    warn!(
                        "Failed to evalueate zone {} and send air filter command: {:?}",
                        zone.name(),
                        e
                    )
                }
            }
            Ok(Update::NotUpdated) => {}
            Err(e) => warn!("Error when updateing zone {}: {:?}", zone.name(), e),
        }
    }
}

async fn update_zones_via_time(zones: &mut [Zone]) {
    debug!("Updating sensors on interval");
    for zone in zones.iter_mut() {
        match zone.update_via_time() {
            Ok(Update::Updated) => {
                if let Err(e) = zone.evaluate_and_send_command().await {
                    warn!(
                        "Failed to evalueate zone {} and send air filter command: {:?}",
                        zone.name(),
                        e
                    )
                }
            }
            Ok(Update::NotUpdated) => {}
            Err(e) => warn!("Error when updateing zone {}: {:?}", zone.name(), e),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Update {
    Updated,
    NotUpdated,
}
