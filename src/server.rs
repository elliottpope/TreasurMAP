// API Outline:

// CommandHandler::new()
// handler.register_command(name, validator, handler)
// ...

// FileStore::new(directory)

// ElasticsearchIndex::new(url).with_auth(username, password)

// PostgresUserStore::new(url, user, password, database)

// Config::default().from_env().from_file(file1).from_file(file2).build()

// Server::new(config).with_handler(handler).with_store(store).with_index(index).with_user_store(user_store)

// server.start()

use std::sync::Arc;
use std::time::Duration;

use async_listen::{error_hint, ListenExt};
use async_lock::RwLock;
use async_std::io::BufReader;
use async_std::net::{TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task::spawn;

use futures::channel::mpsc::unbounded;
use futures::SinkExt;
use log::{info, trace, warn};

use crate::util::{Receiver, Result, Sender};

struct Command {
    tag: String,
    command: String,
    args: Vec<String>,
}

enum ResponseStatus {
    OK,
    BAD,
    NO,
}

struct Response {
    tag: String,
    status: ResponseStatus,
    message: String,
}

#[async_trait::async_trait]
trait HandleCommand {
    fn name<'a>(&self) -> &'a str;
    async fn validate<'a>(&self, command: &'a Command) -> Result<()>;
    async fn handle<'a>(&self, command: &'a Command) -> Result<Response>;
}

struct DelegatingCommandHandler {
    handlers: Arc<RwLock<Vec<Box<dyn HandleCommand + Send + Sync>>>>,
}

#[async_trait::async_trait]
impl HandleCommand for DelegatingCommandHandler {
    fn name<'a>(&self) -> &'a str {
        ""
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        let read_lock = &*self.handlers.read().await;
        for handler in read_lock {
            match handler.validate(&command).await {
                Ok(..) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Response> {
        let read_lock = &*self.handlers.read().await;
        for handler in read_lock {
            match handler.handle(&command).await {
                Ok(response) => return Ok(response),
                Err(..) => continue,
            }
        }
        Ok(Response {
            tag: command.tag.clone(),
            status: ResponseStatus::NO,
            message: "Command unknown".to_string(),
        })
    }
}

impl DelegatingCommandHandler {
    async fn register_command(&self, handler: impl HandleCommand + Send + Sync + 'static) {
        let write_lock = &mut *self.handlers.write().await;
        write_lock.push(Box::new(handler));
    }
}

pub struct ServerConfiguration {
    address: String,
    max_connections: usize,
    error_timeout: Duration,
}

pub struct Configuration {
    server: ServerConfiguration,
}

impl Default for ServerConfiguration {
    fn default() -> Self {
        ServerConfiguration {
            address: "127.0.0.1:3143".to_string(),
            max_connections: 100,
            error_timeout: Duration::from_millis(500),
        }
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            server: ServerConfiguration::default(),
        }
    }
}

#[async_trait::async_trait]
pub trait Server {
    async fn start(&self) -> Result<()>;
}

pub struct DefaultServer {
    config: Configuration,
}

#[async_trait::async_trait]
impl Server for DefaultServer {
    async fn start(&self) -> Result<()> {
        trace!("Server starting on {}", &self.config.server.address);
        let listener = TcpListener::bind(&self.config.server.address).await?;

        println!("Preparing to listen for connections ...");
        let mut incoming = listener
            .incoming()
            .log_warnings(|e| {
                warn!(
                    "An error ocurred while accepting a new connection {}. {}",
                    e,
                    error_hint(&e)
                )
            })
            .handle_errors(self.config.server.error_timeout)
            .backpressure(self.config.server.max_connections);
        while let Some((token, socket)) = incoming.next().await {
            println!("New connection ...");
            spawn(async move {
                let _holder = token;
                new_connection(socket)
            });
        }

        Ok(())
    }
}

impl DefaultServer {
    pub fn new(config: Configuration) -> Self {
        DefaultServer { config }
    }

    pub async fn start_with_notification(&self, sender: std::sync::mpsc::Sender<()>) -> Result<()> {
        trace!("Server starting on {}", &self.config.server.address);
        let listener = TcpListener::bind(&self.config.server.address).await?;

        trace!(
            "Listening for connections on {}",
            &self.config.server.address
        );
        let mut incoming = listener
            .incoming()
            .log_warnings(|e| {
                warn!(
                    "An error ocurred while accepting a new connection {}. {}",
                    e,
                    error_hint(&e)
                )
            })
            .handle_errors(self.config.server.error_timeout)
            .backpressure(self.config.server.max_connections);
        sender.send(())?;
        drop(sender);
        while let Some((token, socket)) = incoming.next().await {
            trace!("New connection from {}", &socket.peer_addr()?);
            spawn(async move {
                let _holder = token;
                trace!(
                    "Spawning handler for new connection from {}",
                    &socket.peer_addr()?
                );
                new_connection(socket).await
            });
        }

        Ok(())
    }
}

async fn new_connection(stream: TcpStream) -> Result<()> {
    let stream = Arc::new(stream);
    let output = Arc::clone(&stream);
    let (mut response_sender, mut response_receiver): (Sender<String>, Receiver<String>) =
        unbounded();
    trace!(
        "Spawning writer thread for connection from {}",
        &stream.peer_addr()?
    );
    let writer = spawn(async move {
        let mut output = &*output;
        while let Some(response) = response_receiver.next().await {
            output.write(response.as_bytes()).await.unwrap();
            output.write("\n".as_bytes()).await.unwrap();
        }
    });
    info!("Sending greeting to client at {}", &stream.peer_addr()?);
    response_sender
        .send("* OK IMAP4rev2 server ready".to_string())
        .await?;
    let input = BufReader::new(&*stream);
    let mut lines = input.lines();
    trace!(
        "Reading input from connection at {}",
        &stream.peer_addr().unwrap()
    );
    while let Some(line) = lines.next().await {
        // let command = parse(line);
        // let response = handle(command);
        trace!("Read from client at {}", &stream.peer_addr().unwrap());
        response_sender.send(line.unwrap()).await.unwrap();
    }
    drop(response_sender);
    writer.await;
    Ok(())
}
