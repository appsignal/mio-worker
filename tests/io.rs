use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use log::*;
use mio::net::{TcpListener, TcpStream};
use mio::{event::Event, Interest, Poll, Registry, Token};
use mio_worker::{Handler, Result, WorkerContext};

mod common;

type BytesRead = Arc<AtomicUsize>;

const SERVER: Token = Token(0);

struct ServerHandler {
    listener: TcpListener,
    connections: HashMap<Token, TcpStream>,
    latest_token: Token,
    pub bytes_read: BytesRead,
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
                    // Probably read from a connection
                    let mut received_data = vec![0; 4096];
                    match connection.read(&mut received_data[0..]) {
                        Ok(bytes_read) => {
                            trace!("Read {} bytes", bytes_read);
                            self.bytes_read.fetch_add(bytes_read, Ordering::SeqCst);
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
    pub fn new(listener: TcpListener, bytes_read: BytesRead) -> Self {
        Self {
            listener,
            connections: HashMap::new(),
            latest_token: Token(SERVER.0 + 1),
            bytes_read,
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
            if self.message_count > 499 {
                return Ok(());
            }
            // Make a message and write it
            let message = b"aaaaaaaaaa";
            trace!("Writing message: {}", self.message_count);
            self.stream.write(message).unwrap();
            self.message_count += 1;
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

    // Set up the TCP server
    let mut listener = TcpListener::bind(address).unwrap();
    let server_poll = Poll::new().unwrap();
    server_poll
        .registry()
        .register(&mut listener, SERVER, Interest::READABLE)
        .unwrap();

    // Create a bytes read store and handler
    let bytes_read = Arc::new(AtomicUsize::new(0));
    let server_handler = ServerHandler::new(listener, bytes_read.clone());

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
    let client_poll = Poll::new().unwrap();
    client_poll
        .registry()
        .register(&mut stream, Token(0), Interest::WRITABLE)
        .unwrap();

    // Create a handler
    let client_handler = ClientHandler::new(stream);

    let client_context = WorkerContext::new(64);
    let mut client_worker = client_context
        .create_worker(client_poll, client_handler)
        .unwrap()
        .unwrap();
    thread::spawn(move || {
        client_worker.run().unwrap();
    });

    client_context.shutdown().unwrap();
    server_context.shutdown().unwrap();

    // Sleep for a bit
    thread::sleep(Duration::from_secs(1));

    // See if we received the messages
    assert_eq!(5000, bytes_read.load(Ordering::SeqCst));
}
