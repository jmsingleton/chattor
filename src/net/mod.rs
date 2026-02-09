// Network module - connection and delivery management

pub mod queue;
pub mod listener;

pub use queue::MessageQueue;
pub use listener::{listen_for_connections, IncomingMessage};
