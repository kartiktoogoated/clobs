pub mod events;
pub mod inputs;

pub mod kafka_worker;
pub mod matching_loop;
pub mod metrics;
pub mod msgpack;
pub mod orderbook;
pub mod outputs;
pub mod persist;
pub mod routes;
pub mod worker;
use std::sync::atomic::AtomicU32;

pub static ORDER_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
