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
        debug!("Message {:?}", message);
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

    // Send a couple of messages
    for i in 0..5 {
        context.send_message(format!("Message {}", i)).unwrap();
    }

    // Sleep for a bit
    thread::sleep(Duration::from_secs(1));

    // See if we received the messages
    let messages = messages.lock().unwrap();
    assert_eq!(5, messages.len());
    assert_eq!("Message 0", messages[0]);
    assert_eq!("Message 4", messages[4]);
}

#[test]
fn test_messages_with_context() {
    common::setup();

    // Create a content to use later
    let context = Arc::new(WorkerContext::new());

    // Send a couple of messages
    for i in 0..5 {
        context.send_message(format!("Message {}", i)).unwrap();
    }

    // Create a mio poll instance
    let poll = Poll::new().unwrap();

    // Create a messages store and handler
    let messages = Arc::new(Mutex::new(Vec::new()));
    let handler = MessagesTestHandler::new(messages.clone());

    // Create worker and get a context
    let mut worker = Worker::with_context(poll, handler, context).unwrap();

    // Run the worker
    thread::spawn(move || {
        worker.run().unwrap();
    });

    // Sleep for a bit
    thread::sleep(Duration::from_secs(1));

    // See if we received the messages
    let messages = messages.lock().unwrap();
    assert_eq!(5, messages.len());
    assert_eq!("Message 0", messages[0]);
    assert_eq!("Message 4", messages[4]);
}
