use std::collections::BTreeMap;
use std::mem::replace;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::*;

use crate::Handler;

/// To keep track of upcoming timeouts
pub struct Timeouts<H: Handler> {
    /// Store of timeouts and at which instant they should run
    store: Mutex<BTreeMap<Instant, H::Timeout>>,
}

impl<H> Timeouts<H>
where
    H: Handler,
{
    pub fn new() -> Self {
        Self {
            store: Mutex::new(BTreeMap::new()),
        }
    }

    /// Duration to the next timeout
    pub fn next_timeout(&self) -> Option<Duration> {
        match self.store.lock() {
            Ok(store) => match store.iter().next() {
                // Port to https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.first_key_value
                // once this is stabilized.
                Some((instant, _)) => instant.checked_duration_since(Instant::now()),
                None => None,
            },
            Err(e) => {
                error!(
                    "Could not lock timeout store, not returning next timeout: {:?}",
                    e
                );
                None
            }
        }
    }

    /// Set a timeout with a certain duration
    pub fn set(&self, duration: Duration, timeout: H::Timeout) {
        match self.store.lock() {
            Ok(mut store) => {
                // Calculate when we want this to run
                let time = match Instant::now().checked_add(duration) {
                    Some(t) => t,
                    None => {
                        error!("Instant out of range when adding {:?}", duration);
                        return;
                    }
                };
                // Insert into store
                store.insert(time, timeout);
            }
            Err(e) => error!("Could not lock timeout store, dropping: {:?}", e),
        }
    }

    /// Pop the timeouts that are ready to fire
    pub fn pop(&self) -> Option<BTreeMap<Instant, H::Timeout>> {
        match self.store.lock() {
            Ok(mut store) => {
                // Split off everything after this moment
                let after_now = store.split_off(&Instant::now());

                // Place the split off timeouts back in the store and get
                // the ones we are interested in.
                let to_run = replace(&mut *store, after_now);

                if to_run.is_empty() {
                    None
                } else {
                    trace!("Popping {} timeout(s)", to_run.len());
                    Some(to_run)
                }
            }
            Err(e) => {
                error!(
                    "Could not lock timeout store, not popping timeouts: {:?}",
                    e
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    struct MockHandler {}
    impl Handler for MockHandler {
        type Message = ();
        type Timeout = &'static str;
    }

    #[test]
    fn test_timeout() {
        let timeouts: Timeouts<MockHandler> = Timeouts::new();

        timeouts.set(Duration::from_millis(750), "Timeout 2");
        timeouts.set(Duration::from_millis(250), "Timeout 1");

        let next = timeouts.next_timeout().expect("No next timeout");
        assert!(next.as_millis() < 260);
        assert!(next.as_millis() > 240);

        // Empty at first
        assert!(timeouts.pop().is_none());

        // Sleep 300 ms
        thread::sleep(Duration::from_millis(300));

        // First timeout
        let popped = timeouts.pop().expect("No timeout");
        assert_eq!(1, popped.len());
        assert_eq!(&"Timeout 1", popped.values().next().unwrap());

        let next = timeouts.next_timeout().expect("No next timeout");
        assert!(next.as_millis() < 550);
        assert!(next.as_millis() > 250);

        // Sleep 600 ms
        thread::sleep(Duration::from_millis(600));

        // Second timeout
        let popped = timeouts.pop().expect("No timeout");
        assert_eq!(1, popped.len());
        assert_eq!(&"Timeout 2", popped.values().next().unwrap());

        // No timeouts scheduled now
        assert!(timeouts.next_timeout().is_none());
        assert!(timeouts.pop().is_none());
    }
}
