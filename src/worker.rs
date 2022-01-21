use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mio::{Events, Poll, Token, Waker};

use crate::message::Messages;
use crate::timeout::Timeouts;
use crate::{Handler, Result};

use log::{error, trace};

/// Indicates a message was sent or timeout set/triggered
const WAKER_TOKEN: Token = Token(std::usize::MAX);

pub struct WorkerContext<H: Handler> {
    /// Waker to wake the worker when there is a new message or timeout
    waker: Mutex<Option<Waker>>,
    /// Enqueued messages to be delivered to the handler
    messages: Messages<H>,
    /// Timeouts to trigger in the handler
    timeouts: Timeouts<H>,
    /// Whether the worker should be running
    running: AtomicBool
}

impl<H> WorkerContext<H>
where
    H: Handler,
{
    pub fn new() -> Self {
        Self {
            waker: Mutex::new(None),
            messages: Messages::new(),
            timeouts: Timeouts::new(),
            running: AtomicBool::new(false),
        }
    }

    fn with_poll(poll: &Poll) -> Result<Self> {
        let waker = Mutex::new(Some(Waker::new(poll.registry(), WAKER_TOKEN)?));
        Ok(Self {
            waker: waker,
            messages: Messages::new(),
            timeouts: Timeouts::new(),
            running: AtomicBool::new(false),
        })
    }

    /// Send a message to the handler running in this worker
    pub fn send_message(&self, message: H::Message) -> Result<()> {
        trace!("Sending message");
        // Push the message onto the queue
        self.messages.push(message);
        // Wake up the worker
        self.wake()
    }

    /// Set a timeout to run
    pub fn set_timeout(&self, duration: Duration, timeout: H::Timeout) -> Result<()> {
        trace!("Setting timeout for {}ms from now", duration.as_millis());
        // Set the timeout
        self.timeouts.set(duration, timeout);
        // Wake up the worker
        self.wake()
    }

    /// Shutdown the worker this context is bound to
    pub fn shutdown(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.wake()
    }

    /// Whether the worker this context is bound to is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) && self.wake().is_ok()
    }

    fn set_waker(&self, poll: &Poll) -> Result<()> {
        match self.waker.lock() {
            Ok(mut waker) => {
                let new_waker = Waker::new(poll.registry(), WAKER_TOKEN)?;
                waker.replace(new_waker);
            },
            Err(e) => {
                error!("Cannot lock waker: {}", e);
            }
        }
        Ok(())
    }

    fn wake(&self) -> Result<()> {
        match self.waker.lock() {
            Ok(waker) => match waker.as_ref() {
                Some(waker) => waker.wake(),
                None => {
                    trace!("Sending message to context without waker");
                    Ok(())
                }
            },
            Err(e) => {
                error!("Cannot lock waker: {}", e);
                Ok(())
            }
        }
    }
}

pub struct Worker<H: Handler> {
    poll: Poll,
    handler: H,
    context: Arc<WorkerContext<H>>,
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
        let context = Arc::new(WorkerContext::with_poll(&poll)?);
        Ok(Self {
            poll: poll,
            handler: handler,
            context: context,
            events_capacity: 128,
        })
    }

    /// Create a new worker with a context that was already created
    /// earlier. A context can only be used with this function once.
    pub fn with_context(poll: Poll, handler: H, context: Arc<WorkerContext<H>>) -> Result<Self> {
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
    pub fn context(&self) -> Arc<WorkerContext<H>> {
        self.context.clone()
    }

    /// Run this worker, blocks the thread it is on until
    /// it finishes.
    pub fn run(&mut self) -> Result<()> {
        trace!("Starting worker");

        // Store that we're running
        self.context.running.store(true, Ordering::SeqCst);

        // Storage for events
        let mut events = Events::with_capacity(self.events_capacity);

        // Duration for the next poll
        let mut poll_duration: Option<Duration> = None;

        loop {
            // Check that we need to be running
            if !self.context.running.load(Ordering::Relaxed) {
                return Ok(());
            }

            // Poll for new events
            self.poll.poll(&mut events, poll_duration)?;

            // Handle events
            for event in &events {
                if event.token() == WAKER_TOKEN {
                    // We woke because a message was enqueued or timeout set
                } else {
                    // We woke because of an IO event
                    trace!("Triggering ready on handler");
                    self.handler
                        .ready(&self.context, self.poll.registry(), event)?;
                }
            }

            // Handle next message
            match self.context.messages.pop() {
                Some(message) => {
                    trace!("Triggering notify on handler");
                    // Run the handler
                    self.handler
                        .notify(&self.context, self.poll.registry(), message)?;
                    // See if there are more messages we need to wake up for
                    if !self.context.messages.is_empty() {
                        self.context.wake()?;
                    }
                }
                None => trace!("No messages"),
            }

            // Handle timeouts
            match self.context.timeouts.pop() {
                Some(timeouts) => {
                    trace!("Triggering {} timeout(s) in handler", timeouts.len());
                    for (_instant, timeout) in timeouts {
                        self.handler
                            .timeout(&self.context, self.poll.registry(), timeout)?;
                    }
                }
                None => trace!("No timeouts"),
            }

            // Set the poll duration to match up to the next timeout
            poll_duration = self.context.timeouts.next_timeout();
        }
    }

    pub fn take_handler(self) -> H {
        self.handler
    }
}
