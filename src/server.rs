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
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{Display, Formatter};

use async_listen::{error_hint, ListenExt};
use async_std::io::BufReader;
use async_std::net::{TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task::spawn;

use futures::channel::mpsc::unbounded;
use futures::SinkExt;
use log::{info, trace, warn};

use crate::util::{Receiver, Result, Sender};
use crate::handlers::{HandleCommand, DelegatingCommandHandler};

pub struct Command {
    tag: String,
    command: String,
    args: Vec<String>,
}

impl Command {
    pub fn tag(&self) -> String {
        self.tag.clone()
    }
    pub fn command(&self) -> String {
        self.command.clone()
    }
    pub fn arg(&self, position: usize) -> String {
        if self.args.len() <= position {
            return "".to_string()
        }
        self.args[position].clone()
    }
    pub fn num_args(&self) -> usize {
        self.args.len()
    }
}

#[derive(Debug)]
pub enum ResponseStatus {
    OK,
    BAD,
    NO,
}
impl Display for ResponseStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            ResponseStatus::OK => write!(f, "OK"),
            ResponseStatus::BAD => write!(f, "BAD"),
            ResponseStatus::NO => write!(f, "NO"),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    tag: String,
    status: ResponseStatus,
    command: String,
    message: String,
}

impl Response {
    pub fn new(tag: String, status: ResponseStatus, command: String, message: String) -> Response {
        Response {
            tag,
            status,
            command,
            message,
        }
    }
}

impl ToString for Response {
    fn to_string(&self) -> String {
        format!("{} {} {} {}", self.tag, self.status, self.command, self.message)
    }
}

#[derive(Debug)]
struct ParseError;
impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "ParseError")
    }
}
impl Error for ParseError{}

impl Command {
    fn parse(cmd: &String) -> std::result::Result<Command, ParseError> {
        let mut values: VecDeque<String> = cmd.split(" ").map(|s| s.to_string()).collect();
        let tag = match values.pop_front() {
            Some(t) => t,
            None => return Err(ParseError{}),
        };
        let command = match values.pop_front() {
            Some(c) => c,
            None => return Err(ParseError{}),
        };
        Ok(Command {
            tag,
            command,
            args: Vec::from(values),
        })
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
    handler: Arc<DelegatingCommandHandler>,
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
            let handler = self.handler.clone();
            println!("New connection ...");
            spawn(async move {
                let _holder = token;
                new_connection(socket, handler)
            });
        }

        Ok(())
    }
}

impl DefaultServer {
    pub fn new(config: Configuration, handler: DelegatingCommandHandler) -> Self {
        DefaultServer { 
            config,
            handler: Arc::new(handler),
        }
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
            let handler = self.handler.clone();
            spawn(async move {
                let _holder = token;
                trace!(
                    "Spawning handler for new connection from {}",
                    &socket.peer_addr()?
                );
                new_connection(socket, handler).await
            });
        }

        Ok(())
    }
}

async fn new_connection<T: HandleCommand>(stream: TcpStream, handler: Arc<T>) -> Result<()> {
    let stream = Arc::new(stream);
    let output = Arc::clone(&stream);
    let (mut response_sender, mut response_receiver): (Sender<Response>, Receiver<Response>) =
        unbounded();
    trace!(
        "Spawning writer thread for connection from {}",
        &stream.peer_addr()?
    );
    let writer = spawn(async move {
        let mut output = &*output;
        while let Some(response) = response_receiver.next().await {
            output.write(response.to_string().as_bytes()).await.unwrap();
            output.write("\r\n".as_bytes()).await.unwrap();
        }
    });
    info!("Sending greeting to client at {}", &stream.peer_addr()?);
    response_sender.send(Response {
        tag: "*".to_string(),
        status: ResponseStatus::OK,
        command: "IMAP4rev2".to_string(),
        message: "server ready".to_string(),
    }).await?;
    let input = BufReader::new(&*stream);
    let mut lines = input.lines();
    trace!(
        "Reading input from connection at {}",
        &stream.peer_addr().unwrap()
    );
    while let Some(line) = lines.next().await {
        let line = line?;
        println!("Read {} from client at {}", &line, &stream.peer_addr().unwrap());
        let command = Command::parse(&line)?;
        let response = handler.handle(&command).await?;
        println!("Sending {} to client at {}", &response.to_string(), &stream.peer_addr().unwrap());
        response_sender.send(response).await.unwrap();
    }
    drop(response_sender);
    writer.await;
    Ok(())
}
