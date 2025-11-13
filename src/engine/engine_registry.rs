use crate::events::OrderEvent;
use crate::outputs::Depth;
use parking_lot::RwLock;
use ringbuf::HeapProd;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct EngineHandle {
    pub market: String,
    pub tx: HeapProd<OrderEvent>,
    pub depth_snapshot: Arc<RwLock<Depth>>,
}

pub struct EngineRegistry {
    engines: Arc<RwLock<HashMap<String, EngineHandle>>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self {
            engines: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, handle: EngineHandle) {
        self.engines.write().insert(handle.market.clone(), handle);
    }

    pub fn route(&self, event: OrderEvent) -> Result<(), &'static str> {
        let market = match &event {
            OrderEvent::NewOrder { market, .. } => market,
            OrderEvent::DeleteOrder { market, .. } => market,
        };

        let engines = self.engines.read();
        let engine = engines.get(market).ok_or("Market not found")?;

        engine.tx.try_push(event).map_err(|_| "Queue full")?;

        Ok(())
    }

    pub fn get_depth(&self, market: &str) -> Option<Depth> {
        let engines = self.engines.read();
        engines.get(market).map(|handle| {
            let snapshot = handle.depth_snapshot.read();
            Depth {
                bids: snapshot.bids.clone(),
                asks: snapshot.asks.clone(),
                lastUpdateId: snapshot.lastUpdateId.clone(),
            }
        })
    }

    pub fn markets(&self) -> Vec<String> {
        self.engines.read().keys().cloned().collect()
    }
}

impl Clone for EngineRegistry {
    fn clone(&self) -> Self {
        Self {
            engines: Arc::clone(&self.engines),
        }
    }
}
