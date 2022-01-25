use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use log::*;
use mio::{Poll, Registry};
use mio_worker::{Handler, Result, Worker, WorkerContext};

mod common;

type Messages = Arc<Mutex<Vec<String>>>;

struct MessagesTestHandler {
    pub messages: Messages,
}

impl Handler for MessagesTestHandler {
    type Message = String;
    type Timeout = ();

    fn notify(
        &mut self,
        _context: &WorkerContext<Self>,
        _registry: &Registry,
        message: Self::Message,
    ) -> Result<()> {
        debug!("Handler message call {:?}", message);
        self.messages.lock().unwrap().push(message);
        Ok(())
    }
}

impl MessagesTestHandler {
    pub fn new(messages: Messages) -> Self {
        Self { messages: messages }
    }
}

#[test]
fn test_messages_new() {
    common::setup();

    // Create a mio poll instance
    let poll = Poll::new().unwrap();

    // Create a messages store and handler
    let messages = Arc::new(Mutex::new(Vec::new()));
    let handler = MessagesTestHandler::new(messages.clone());

    // Create worker and get a context
    let mut worker = Worker::new(poll, handler).unwrap();
    let context = worker.context();

    // Run the worker
    thread::spawn(move || {
        worker.run().unwrap();
    });

    // Send messages. Both fast and with some time in between.
    for i in 0..10 {
        thread::sleep(Duration::from_millis(100));
        for i2 in 0..10_000 {
            context
                .send_message(format!("Message {}-{}", i, i2))
                .unwrap();
        }
    }

    // Sleep for a bit
    thread::sleep(Duration::from_secs(1));

    // See if we received the messages
    let messages = messages.lock().unwrap();
    assert_eq!(100_000, messages.len());
    assert_eq!("Message 0-0", messages[0]);
    assert_eq!("Message 0-4", messages[4]);
}

#[test]
fn test_messages_with_context() {
    common::setup();

    // Create a content to use later
    let context = Arc::new(WorkerContext::new());

    // Should not be running yet
    assert!(!context.is_running());

    // Send a couple of messages
    for i in 0..10_000 {
        context.send_message(format!("Message {}", i)).unwrap();
    }

    // Create a mio poll instance
    let poll = Poll::new().unwrap();

    // Create a messages store and handler
    let messages = Arc::new(Mutex::new(Vec::new()));
    let handler = MessagesTestHandler::new(messages.clone());

    // Create worker and get a context
    let mut worker = Worker::with_context(poll, handler, context.clone()).unwrap();

    // Run the worker
    thread::spawn(move || {
        worker.run().unwrap();
    });

    // Sleep for a bit
    thread::sleep(Duration::from_millis(500));

    assert!(context.is_running());

    // Shutdown the worker
    context.shutdown().expect("Could not shutdown");

    // Sleep for a bit
    thread::sleep(Duration::from_millis(500));

    // Worker should not be running now
    assert!(!context.is_running());

    // See if we received the messages
    let messages = messages.lock().unwrap();
    assert_eq!(10_000, messages.len());
    assert_eq!("Message 0", messages[0]);
    assert_eq!("Message 4", messages[4]);
}
