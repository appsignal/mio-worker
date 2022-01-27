use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use mio::{Poll, Waker};

use crate::message::Messages;
use crate::timeout::Timeouts;
use crate::{Handler, Result, WAKER_TOKEN};

use log::{error, trace};

pub struct WorkerContext<H: Handler> {
    /// Waker to wake the worker when there is a new message or timeout
    waker: Mutex<Option<Waker>>,
    /// Enqueued messages to be delivered to the handler
    pub messages: Messages<H>,
    /// Timeouts to trigger in the handler
    pub timeouts: Timeouts<H>,
    /// Whether the worker should be running
    pub running: AtomicBool,
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

    pub fn with_poll(poll: &Poll) -> Result<Self> {
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
        trace!("Sending message {:?}", message);
        // Push the message onto the queue
        self.messages.push(message);
        // Wake up the worker
        self.wake()
    }

    /// Set a timeout to run
    pub fn set_timeout(&self, duration: Duration, timeout: H::Timeout) -> Result<()> {
        trace!(
            "Setting timeout {:?} for {}ms from now",
            timeout,
            duration.as_millis()
        );
        // Set the timeout
        self.timeouts.set(duration, timeout);
        // Waking up the worker ensures that the right timeout is used
        // for the next poll
        self.wake()
    }

    /// Shutdown the worker this context is bound to
    pub fn shutdown(&self) -> Result<()> {
        trace!("Called shutdown on worker context");
        self.running.store(false, Ordering::SeqCst);
        self.wake()
    }

    /// Whether the worker this context is bound to is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) && self.wake().is_ok()
    }

    pub fn set_waker(&self, poll: &Poll) -> Result<()> {
        match self.waker.lock() {
            Ok(mut waker) => {
                let new_waker = Waker::new(poll.registry(), WAKER_TOKEN)?;
                waker.replace(new_waker);
            }
            Err(e) => {
                error!("Cannot lock waker: {}", e);
            }
        }
        Ok(())
    }

    pub fn wake(&self) -> Result<()> {
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
