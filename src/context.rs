use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mio::{Poll, Waker};

use crate::message::Messages;
use crate::timeout::Timeouts;
use crate::{Handler, Result, WAKER_TOKEN, Worker};

use log::{error, trace};

pub struct WorkerContext<H: Handler> {
    inner: Arc<WorkerContextInner<H>>
}

struct WorkerContextInner<H: Handler> {
    /// Waker to wake the worker when there is a new message or timeout
    waker: Mutex<Option<Waker>>,
    /// Enqueued messages to be delivered to the handler
    messages: Messages<H>,
    /// Timeouts to trigger in the handler
    timeouts: Timeouts<H>,
    /// Whether the worker should be running
    running: AtomicBool,
    /// Whether a worker has been created
    worker_created: AtomicBool
}

impl<H> WorkerContext<H>
where
    H: Handler,
{
    /// Create a new worker context
    pub fn new() -> Self {
        Self {
            inner: Arc::new(WorkerContextInner {
                waker: Mutex::new(None),
                messages: Messages::new(),
                timeouts: Timeouts::new(),
                running: AtomicBool::new(false),
                worker_created: AtomicBool::new(false),
            })
        }
    }

    /// Create a worker out of this context. This can only be done once, this returns
    /// `None` if a worker has already been created with this context.
    pub fn create_worker(&self, poll: Poll, handler: H) -> Option<Result<Worker<H>>> {
        if self.worker_created() {
            return None
        }
        match self.set_waker(&poll) {
            Ok(()) => (),
            Err(e) => return Some(Err(e))
        }
        match self.wake() {
            Ok(()) => (),
            Err(e) => return Some(Err(e))
        }
        self.inner.worker_created.store(true, Ordering::SeqCst);
        Some(Worker::new(poll, handler, self.clone()))
    }

    pub fn clone(&self) -> Self {
        WorkerContext { inner: self.inner.clone() }
    }

    /// Send a message to the handler running in this worker
    pub fn send_message(&self, message: H::Message) -> Result<()> {
        trace!("Sending message {:?}", message);
        // Push the message onto the queue
        self.inner.messages.push(message);
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
        self.inner.timeouts.set(duration, timeout);
        // Waking up the worker ensures that the right timeout is used
        // for the next poll
        self.wake()
    }

    /// Shutdown the worker this context is bound to
    pub fn shutdown(&self) -> Result<()> {
        trace!("Called shutdown on worker context");
        self.inner.running.store(false, Ordering::SeqCst);
        self.wake()
    }

    /// Whether a worker has been created out of this context
    pub fn worker_created(&self) -> bool {
        self.inner.worker_created.load(Ordering::Relaxed)
    }

    /// Whether the worker this context is bound to is running
    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Relaxed) && self.wake().is_ok()
    }

    /// Whether the worker this context should be running
    pub fn should_run(&self) -> bool {
        self.inner.running.load(Ordering::Relaxed)
    }

    /// Set whether this worker should be running
    pub(crate) fn set_running(&self, running: bool) {
        self.inner.running.store(running, Ordering::SeqCst)
    }

    fn set_waker(&self, poll: &Poll) -> Result<()> {
        match self.inner.waker.lock() {
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
        match self.inner.waker.lock() {
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

    pub(crate) fn messages(&self) -> &Messages<H> {
        &self.inner.messages
    }

    pub(crate) fn timeouts(&self) -> &Timeouts<H> {
        &self.inner.timeouts
    }
}
