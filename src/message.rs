use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use log::*;

use crate::Handler;

pub struct Messages<H: Handler> {
    queue: Mutex<VecDeque<H::Message>>,
    length: AtomicUsize,
}

impl<H> Messages<H>
where
    H: Handler,
{
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            length: AtomicUsize::new(0),
        }
    }

    /// Length of the current queue
    pub fn len(&self) -> usize {
        self.length.load(Ordering::Relaxed)
    }

    /// Whether there are any messages present
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Add a message to this queue
    pub fn push(&self, message: H::Message) {
        match self.queue.lock() {
            Ok(mut queue) => {
                // Add message to the queue
                queue.push_back(message);
                // Store the current queue length
                self.length.store(queue.len(), Ordering::Relaxed);
            }
            Err(e) => error!("Could not lock messages, dropping: {:?}", e),
        }
    }

    /// Pop a message of this queue
    pub fn pop(&self) -> Option<H::Message> {
        if self.is_empty() {
            None
        } else {
            match self.queue.lock() {
                Ok(mut queue) => {
                    let len = self.length.fetch_sub(1, Ordering::Relaxed);
                    trace!("Popping message from queue, {} left", len - 1);
                    queue.pop_front()
                }
                Err(e) => {
                    error!("Could not lock messages, not returning one: {:?}", e);
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHandler {}
    impl Handler for MockHandler {
        type Message = &'static str;
        type Timeout = ();
    }

    #[test]
    fn test_messages() {
        let messages: Messages<MockHandler> = Messages::new();

        assert!(messages.is_empty());
        assert_eq!(0, messages.len());

        messages.push("Message 1");
        messages.push("Message 2");
        messages.push("Message 3");

        assert!(!messages.is_empty());
        assert_eq!(3, messages.len());

        assert_eq!(Some("Message 1"), messages.pop());
        assert!(!messages.is_empty());
        assert_eq!(2, messages.len());

        assert_eq!(Some("Message 2"), messages.pop());
        assert!(!messages.is_empty());
        assert_eq!(1, messages.len());

        assert_eq!(Some("Message 3"), messages.pop());
        assert!(messages.is_empty());
        assert_eq!(0, messages.len());
    }
}
