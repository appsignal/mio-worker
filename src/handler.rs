use mio::event::Event;
use mio::Registry;

use crate::{Result, WorkerContext};

pub trait Handler
where
    Self: Sized,
{
    type Message;
    type Timeout;

    /// Handle a message that was sent to the worker context
    fn notify(
        &mut self,
        _context: &WorkerContext<Self>,
        _registry: &Registry,
        _message: Self::Message,
    ) -> Result<()> {
        Ok(())
    }

    /// Handle a timeout that was set in the worker context
    fn timeout(
        &mut self,
        _context: &WorkerContext<Self>,
        _registry: &Registry,
        _timeout: Self::Timeout,
    ) -> Result<()> {
        Ok(())
    }

    /// Handle an IO ready event
    fn ready(
        &mut self,
        _context: &WorkerContext<Self>,
        _registry: &Registry,
        _event: &Event,
    ) -> Result<()> {
        Ok(())
    }
}
