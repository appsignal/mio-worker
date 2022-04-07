use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use log::*;
use mio::net::{TcpListener, TcpStream};
use mio::{event::Event, Interest, Poll, Registry, Token};
use mio_worker::{Handler, Result, WorkerContext};

mod common;

type BytesReceived = Arc<AtomicUsize>;

const SERVER: Token = Token(0);

struct ServerHandler {
    listener: TcpListener,
    connections: HashMap<Token, TcpStream>,
    latest_token: Token,
    bytes_received: BytesReceived,
}

impl Handler for ServerHandler {
    type Message = String;
    type Timeout = ();

    fn ready(
        &mut self,
        _context: &WorkerContext<Self>,
        registry: &Registry,
        event: &Event,
    ) -> Result<()> {
        match event.token() {
            SERVER => loop {
                let (mut connection, address) = match self.listener.accept() {
                    Ok((connection, address)) => (connection, address),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e),
                };

                info!("Accepted connection from {}", address);

                // Increment token
                self.latest_token.0 += 1;

                // Register interest with new token
                registry.register(&mut connection, self.latest_token, Interest::READABLE)?;

                // Store connection
                self.connections.insert(self.latest_token, connection);
            },
            token => match self.connections.get_mut(&token) {
                Some(connection) if event.is_readable() => {
                    let mut received_data = vec![0; 4096];
                    match connection.read(&mut received_data) {
                        Ok(bytes_read) => {
                            // Receive message and add it
                            let message =
                                String::from_utf8_lossy(&received_data[bytes_read..]).to_string();
                            debug!("Received data: '{}'", message);
                            self.bytes_received.fetch_add(bytes_read, Ordering::SeqCst);
                        }
                        Err(e) => error!("Error reading: {:?}", e),
                    }
                }
                _ => (),
            },
        }

        Ok(())
    }
}

impl ServerHandler {
    pub fn new(listener: TcpListener, bytes_received: BytesReceived) -> Self {
        Self {
            listener,
            connections: HashMap::new(),
            latest_token: Token(SERVER.0 + 1),
            bytes_received,
        }
    }
}

struct ClientHandler {
    stream: TcpStream,
    message_count: usize,
}

impl Handler for ClientHandler {
    type Message = ();
    type Timeout = ();

    fn ready(
        &mut self,
        _context: &WorkerContext<Self>,
        _registry: &Registry,
        event: &Event,
    ) -> Result<()> {
        if event.is_writable() {
            // Just write 5
            if self.message_count > 4 {
                return Ok(());
            }
            // Make a message and write it
            self.stream.write(b"aaaaaaaaaa").expect("Could not write");
            self.message_count += 1;
            // Sleep a bit to cheat the messages into distinct reads
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

impl ClientHandler {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            message_count: 0,
        }
    }
}

#[test]
fn test_io() {
    common::setup();

    // Server address
    let address = "127.0.0.1:9000".parse().unwrap();

    // Create mio poll instances
    let server_poll = Poll::new().unwrap();
    let client_poll = Poll::new().unwrap();

    // Set up the TCP server
    let mut listener = TcpListener::bind(address).unwrap();
    server_poll
        .registry()
        .register(&mut listener, SERVER, Interest::READABLE)
        .unwrap();

    // Create a messages store and handler
    let bytes_received = Arc::new(AtomicUsize::new(0));
    let server_handler = ServerHandler::new(listener, bytes_received.clone());

    // Create a server in a thread
    let server_context = WorkerContext::new(64);
    let mut server_worker = server_context
        .create_worker(server_poll, server_handler)
        .unwrap()
        .unwrap();
    thread::spawn(move || {
        server_worker.run().unwrap();
    });

    // Set up the TCP client
    let mut stream = TcpStream::connect(address).unwrap();
    client_poll
        .registry()
        .register(&mut stream, Token(0), Interest::WRITABLE)
        .unwrap();

    // Create a handler
    let client_handler = ClientHandler::new(stream);

    // Create a client in a thread
    let client_context = WorkerContext::new(64);
    let mut client_worker = client_context
        .create_worker(client_poll, client_handler)
        .unwrap()
        .unwrap();
    thread::spawn(move || {
        client_worker.run().unwrap();
    });

    // Sleep for a bit
    thread::sleep(Duration::from_secs(1));

    // See if we received the correct amount of data
    assert_eq!(50, bytes_received.load(Ordering::SeqCst));
}
