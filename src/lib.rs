mod context;
mod handler;
mod message;
mod timeout;
mod worker;

pub use context::WorkerContext;
pub use handler::Handler;
pub use worker::Worker;

use mio::Token;

pub type Result<T> = std::io::Result<T>;

/// Indicates a message was sent or timeout set/triggered
const WAKER_TOKEN: Token = Token(std::usize::MAX);
