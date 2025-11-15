use crate::persist::event::PersistEvent;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use tokio::sync::mpsc::UnboundedReceiver;

pub async fn start_kafka_producer_worker(
    mut rx: UnboundedReceiver<PersistEvent>,
    producer: FutureProducer,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match wincode::serialize(&event) {
                Ok(payload) => {
                    let topic = match event {
                        PersistEvent::TradeExecuted { .. } => "trades",
                        _ => "orders",
                    };
                    if let Err((e, _)) = producer
                        .send(
                            FutureRecord::to(topic).payload(&payload).key("clob-event"),
                            Timeout::Never,
                        )
                        .await
                    {
                        eprintln!("[KAFKA PRODUCER] Error sending to {}: {:?}", topic, e);
                    }
                }
                Err(e) => {
                    eprintln!("[KAFKA PRODUCER] Serialization error: {:?}", e);
                }
            }
        }
    });
}
