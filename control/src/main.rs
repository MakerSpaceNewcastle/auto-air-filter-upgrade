use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut mqttoptions = MqttOptions::new("rumqtt-doot-test", "100.92.108.55", 1883);
    mqttoptions.set_credentials("dan", "");
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (mut client, mut connection) = AsyncClient::new(mqttoptions, 10);
    client.subscribe("#", QoS::ExactlyOnce).await.unwrap();

    loop {
        let fuck = connection.poll().await;
        println!("{:#?}", fuck);
    }
}
