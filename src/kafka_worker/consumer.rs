use crate::persist::{client::ScyllaClient, event::PersistEvent};
use rdkafka::Message;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::Duration;

pub async fn start_kafka_producer_worker(
    mut rx: UnboundedReceiver<PersistEvent>,
    producer: FutureProducer,
) {
    while let Some(event) = rx.recv().await {
        if let Ok(payload) = wincode::serialize(&event) {
            let topic = match event {
                PersistEvent::TradeExecuted { .. } => "trades",
                _ => "orders",
            };

            let record = FutureRecord::to(topic).payload(&payload).key("clob-event");

            if let Err((e, _)) = producer
                .send(record, Timeout::After(Duration::from_secs(5)))
                .await
            {
                eprintln!("[KAFKA] Send error: {:?}", e);
            }
        }
    }
}

pub async fn start_kafka_consumer_worker(scylla: std::sync::Arc<ScyllaClient>) {
    let consumer: StreamConsumer = rdkafka::config::ClientConfig::new()
        .set("group.id", "clob-consumer")
        .set("bootstrap.servers", "localhost:9092")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .set("session.timeout.ms", "6000")
        .create()
        .expect("Failed to create Kafka consumer");

    consumer
        .subscribe(&["orders", "trades"])
        .expect("Failed to subscribe to topics");

    while let Ok(msg) = consumer.recv().await {
        if let Some(payload) = msg.payload() {
            if let Ok(event) = wincode::deserialize::<PersistEvent>(payload) {
                scylla.handle_event(event).await;
            }
        }
    }
}
