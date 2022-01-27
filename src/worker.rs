use std::time::Duration;

use mio::{Events, Poll};

use crate::context::WorkerContext;
use crate::{Handler, Result, WAKER_TOKEN};

use log::trace;

pub struct Worker<H: Handler> {
    poll: Poll,
    handler: H,
    context: WorkerContext<H>,
    events_capacity: usize,
}

impl<H> Worker<H>
where
    H: Handler,
{
    /// Create a new worker. Pass in a poll that has
    /// any IO you're interested in already registered to it.
    /// Implement the handler trait to get the behaviour you like.
    pub fn new(poll: Poll, handler: H) -> Result<Self> {
        let context = WorkerContext::with_poll(&poll)?;
        Ok(Self {
            poll: poll,
            handler: handler,
            context: context,
            events_capacity: 128,
        })
    }

    /// Create a new worker with a context that was already created
    /// earlier. A context can only be used with this function once.
    pub fn with_context(poll: Poll, handler: H, context: WorkerContext<H>) -> Result<Self> {
        context.set_waker(&poll)?;
        context.wake()?;
        Ok(Self {
            poll: poll,
            handler: handler,
            context: context,
            events_capacity: 128,
        })
    }

    /// Set the events capacity
    pub fn set_events_capacity(&mut self, capacity: usize) {
        self.events_capacity = capacity;
    }

    /// Get an instance of the worker context
    pub fn context(&self) -> WorkerContext<H> {
        self.context.clone()
    }

    /// Run this worker, blocks the thread it is on until
    /// it finishes.
    pub fn run(&mut self) -> Result<()> {
        trace!("Starting worker");

        // Store that we're running
        self.context.set_running(true);

        // Storage for events
        let mut events = Events::with_capacity(self.events_capacity);

        // Duration for the next poll
        let mut poll_duration: Option<Duration> = None;

        loop {
            // Check that we need to be running
            if !self.context.should_run() {
                trace!("Exiting worker loop");
                return Ok(());
            }

            // Handle timeouts
            match self.context.timeouts().pop() {
                Some(timeouts) => {
                    for (_instant, timeout) in timeouts {
                        trace!("Triggering timeout with {:?} on handler", timeout);
                        self.handler
                            .timeout(&self.context, self.poll.registry(), timeout)?;
                    }
                }
                None => (),
            }

            // Poll for new events
            self.poll.poll(&mut events, poll_duration)?;

            // Handle events
            for event in &events {
                if event.token() == WAKER_TOKEN {
                    // We woke because a message was enqueued or a timeout was set
                    match self.context.messages().pop() {
                        Some(message) => {
                            trace!("Triggering notify with {:?} on handler", message);
                            // Run the handler
                            self.handler
                                .notify(&self.context, self.poll.registry(), message)?;
                            // See if there are more messages we need to wake up for
                            if !self.context.messages().is_empty() {
                                self.context.wake()?;
                            }
                        }
                        None => (),
                    }
                } else {
                    if event.is_readable() || event.is_writable() || event.is_error() {
                        // We woke because of an IO event
                        trace!("Triggering ready with token {} on handler", event.token().0);
                        self.handler
                            .ready(&self.context, self.poll.registry(), event)?;
                    }
                }
            }

            // Set the poll duration to match up to the next timeout
            let next_timeout = self.context.timeouts().next_timeout();
            match next_timeout {
                Some(timeout) => {
                    trace!("Setting next poll duration to {}ms", timeout.as_millis())
                }
                None => trace!("Setting next poll duration to none"),
            };
            poll_duration = self.context.timeouts().next_timeout();
        }
    }

    pub fn take_handler(self) -> H {
        self.handler
    }
}
