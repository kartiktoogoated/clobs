use crate::persist::{client::ScyllaClient, event::PersistEvent};
use rdkafka::Message;
use rdkafka::consumer::{Consumer, StreamConsumer};

pub async fn start_kafka_consumer_worker(scylla: ScyllaClient) {
    let consumer: StreamConsumer = rdkafka::config::ClientConfig::new()
        .set("group.id", "clob-consumer")
        .set("bootstrap.servers", "localhost:9092")
        .set("enable.auto.commit", "true")
        .create()
        .expect("Failed to create Kafka consumer");

    consumer
        .subscribe(&["orders", "trades"])
        .expect("Failed to subscribe to topics");

    println!("[KAFKA CONSUMER] Subscribed to 'orders' and 'trades'");

    while let Ok(msg) = consumer.recv().await {
        if let Some(payload_bytes) = msg.payload() {
            match std::str::from_utf8(payload_bytes) {
                Ok(payload) => match serde_json::from_str::<PersistEvent>(payload) {
                    Ok(event) => {
                        println!("[KAFKA CONSUMER] Processing event: {:?}", event);
                        scylla.handle_event(event).await;
                    }
                    Err(e) => {
                        eprintln!(
                            "[KAFKA CONSUMER] Invalid JSON payload: {:?}, error: {:?}",
                            payload, e
                        );
                    }
                },
                Err(e) => {
                    eprintln!("[KAFKA CONSUMER] UTF-8 decode error: {:?}", e);
                }
            }
        } else {
            eprintln!("[KAFKA CONSUMER] Empty Kafka payload");
        }
    }
}
