use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use log::*;
use mio::Poll;
use mio_worker::{Handler, Result, Worker, WorkerContext};

mod common;

type Timeouts = Arc<Mutex<Vec<String>>>;

struct TimeoutsTestHandler {
    pub timeouts: Timeouts,
}

impl Handler for TimeoutsTestHandler {
    type Message = ();
    type Timeout = String;

    fn timeout(&mut self, _context: &WorkerContext<Self>, timeout: Self::Timeout) -> Result<()> {
        debug!("Timeout {:?}", timeout);
        self.timeouts.lock().unwrap().push(timeout);
        Ok(())
    }
}

impl TimeoutsTestHandler {
    pub fn new(timeouts: Timeouts) -> Self {
        Self { timeouts: timeouts }
    }
}

#[test]
fn test_timeout() {
    common::setup();

    // Create a mio poll instance
    let poll = Poll::new().unwrap();

    // Create a messages store and handler
    let timeouts = Arc::new(Mutex::new(Vec::new()));
    let handler = TimeoutsTestHandler::new(timeouts.clone());

    // Create worker and get a context
    let worker = Worker::new(poll, handler).unwrap();
    let context = worker.context();

    // Run the worker
    thread::spawn(move || {
        worker.run().unwrap();
    });

    // Set a couple of timeouts
    for i in 0..5 {
        context
            .set_timeout(Duration::from_millis(i * 100), format!("Timeout {}", i))
            .unwrap();
    }

    // Sleep for a bit
    thread::sleep(Duration::from_secs(2));

    // See if the timeouts triggered
    let timeouts = timeouts.lock().unwrap();
    assert_eq!(5, timeouts.len());
    assert_eq!("Timeout 0", timeouts[0]);
    assert_eq!("Timeout 4", timeouts[4]);
}
