// use crate::events::OrderEvent;
// use crate::outputs::Depth;
// use parking_lot::{Mutex, RwLock};
// use ringbuf::HeapProd;
// use ringbuf::traits::{Observer, Producer};
// use std::collections::HashMap;
// use std::sync::Arc;

// pub struct EngineHandle {
//     pub market: String,
//     pub tx: Arc<Mutex<HeapProd<OrderEvent>>>, // Wrap in Arc<Mutex> to make it shareable
//     pub depth_snapshot: Arc<RwLock<Depth>>,
// }

// pub struct EngineRegistry {
//     engines: Arc<RwLock<HashMap<String, EngineHandle>>>,
// }

// impl EngineRegistry {
//     pub fn new() -> Self {
//         Self {
//             engines: Arc::new(RwLock::new(HashMap::new())),
//         }
//     }

//     pub fn register(&self, handle: EngineHandle) {
//         self.engines.write().insert(handle.market.clone(), handle);
//     }

//     pub fn route_new_order(
//         &self,
//         market: String,
//         order_id: u32,
//         user_id: u32,
//         price: u32,
//         quantity: u32,
//         side: crate::inputs::Side,
//     ) -> Result<(), &'static str> {
//         let engines = self.engines.read();
//         let engine = engines.get(&market).ok_or("Market not found")?;

//         let event = OrderEvent::NewOrder {
//             order_id,
//             user_id,
//             price,
//             quantity,
//             side,
//         };

//         let mut tx = engine.tx.lock();
//         tx.try_push(event)
//             .map_err(|_| "Queue full - order rejected")?;

//         Ok(())
//     }

//     pub fn route_delete_order(&self, market: String, order_id: u32) -> Result<(), &'static str> {
//         let engines = self.engines.read();
//         let engine = engines.get(&market).ok_or("Market not found")?;

//         let event = OrderEvent::DeleteOrder { order_id };

//         let mut tx = engine.tx.lock();
//         tx.try_push(event)
//             .map_err(|_| "Queue full - order rejected")?;

//         Ok(())
//     }

//     pub fn get_depth(&self, market: &str) -> Option<Depth> {
//         let engines = self.engines.read();
//         engines.get(market).map(|handle| {
//             let snapshot = handle.depth_snapshot.read();
//             Depth {
//                 bids: snapshot.bids.clone(),
//                 asks: snapshot.asks.clone(),
//                 lastUpdateId: snapshot.lastUpdateId.clone(),
//             }
//         })
//     }

//     pub fn markets(&self) -> Vec<String> {
//         self.engines.read().keys().cloned().collect()
//     }

//     pub fn queue_depth(&self, market: &str) -> Option<usize> {
//         let engines = self.engines.read();
//         engines.get(market).map(|handle| {
//             let tx = handle.tx.lock();
//             tx.occupied_len()
//         })
//     }
// }

// impl Clone for EngineRegistry {
//     fn clone(&self) -> Self {
//         Self {
//             engines: Arc::clone(&self.engines),
//         }
//     }
// }
