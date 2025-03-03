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
    handler_type_name: &'static str,
}

impl<H> Worker<H>
where
    H: Handler,
{
    /// Create a new worker with a context that was already created
    /// earlier.
    pub(crate) fn new(
        poll: Poll,
        handler: H,
        context: WorkerContext<H>,
        events_capacity: usize,
        handler_type_name: &'static str,
    ) -> Result<Self> {
        Ok(Self {
            poll,
            handler,
            context,
            events_capacity,
            handler_type_name,
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
        trace!("Starting {} worker", self.handler_type_name);

        // Store that we're running
        self.context.set_running(true);

        // Storage for events
        let mut events = Events::with_capacity(self.events_capacity);

        // Duration for the next poll
        let mut poll_duration: Option<Duration> = None;

        loop {
            // Check that we need to be running
            if !self.context.should_run() {
                trace!("Exiting worker loop for {}", self.handler_type_name);
                return Ok(());
            }

            // Handle timeouts
            if let Some(timeouts) = self.context.timeouts().pop() {
                for (_instant, timeout) in timeouts {
                    trace!(
                        "Triggering timeout with {:?} on {}",
                        timeout,
                        self.handler_type_name
                    );
                    self.handler
                        .timeout(&self.context, self.poll.registry(), timeout)?;
                }
            }

            // Poll for new events
            self.poll.poll(&mut events, poll_duration)?;

            // Handle events
            let registry = self.poll.registry();
            for event in events.iter() {
                if event.token() == WAKER_TOKEN {
                    // We woke because a message was enqueued or a timeout was set
                    if let Some(message) = self.context.messages().pop() {
                        trace!(
                            "Triggering notify with {:?} on {}",
                            message,
                            self.handler_type_name
                        );
                        // Run the handler
                        self.handler
                            .notify(&self.context, self.poll.registry(), message)?;
                        // See if there are more messages we need to wake up for
                        if !self.context.messages().is_empty() {
                            self.context.wake()?;
                        }
                    }
                } else if event.is_readable() || event.is_writable() || event.is_error() {
                    // We woke because of an IO event
                    trace!(
                        "Triggering ready with token {} on {}",
                        event.token().0,
                        self.handler_type_name
                    );
                    self.handler.ready(&self.context, registry, event)?;
                }
            }

            // Set the poll duration to match up to the next timeout
            let next_timeout = self.context.timeouts().next_timeout();
            match next_timeout {
                Some(timeout) => {
                    trace!(
                        "Setting next poll duration to {}ms for {}",
                        timeout.as_millis(),
                        self.handler_type_name
                    )
                }
                None => trace!(
                    "Setting next poll duration to none for {}",
                    self.handler_type_name
                ),
            };
            poll_duration = self.context.timeouts().next_timeout();
        }
    }

    pub fn take_handler(self) -> H {
        self.handler
    }
}
