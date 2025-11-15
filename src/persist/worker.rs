use crate::persist::client::ScyllaClient;
use crate::persist::event::PersistEvent;
use tokio::sync::mpsc::UnboundedReceiver;

pub async fn start_persistence_worker(
    mut rx: UnboundedReceiver<PersistEvent>,
    scylla: ScyllaClient,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                PersistEvent::NewOrder(order) => {
                    println!("[Persist] New Order: {:?}", order);
                    if let Err(e) = scylla.insert_order(order).await {
                        eprintln!("Failed to persist order: {:?}", e);
                    }
                }
                PersistEvent::OrderDeleted { order_id } => {
                    println!("[Persist] Order Deleted: {}", order_id);
                    if let Err(e) = scylla.delete_order(order_id).await {
                        eprintln!("Failed to delete order: {:?}", e);
                    }
                }
                PersistEvent::OrderFilled {
                    order_id,
                    traded_qty,
                } => {
                    println!(
                        "[Persist] Order filled: id={}, qty={}",
                        order_id, traded_qty
                    );
                    if let Err(e) = scylla.mark_filled(order_id, traded_qty).await {
                        eprintln!("Failed to mark order filled: {:?}", e);
                    }
                }
                PersistEvent::TradeExecuted {
                    trade_id,
                    price,
                    quantity,
                    maker_order_id,
                    taker_order_id,
                    timestamp,
                } => {
                    println!(
                        "[Persist] Trade executed: price={}, qty={}, maker={}, taker={}",
                        price, quantity, maker_order_id, taker_order_id
                    );

                    if let Err(e) = scylla
                        .insert_trade(
                            trade_id,
                            price,
                            quantity,
                            maker_order_id,
                            taker_order_id,
                            timestamp,
                        )
                        .await
                    {
                        eprintln!("Failed to persist trade: {:?}", e);
                    }
                }
            }
        }
    });
}
