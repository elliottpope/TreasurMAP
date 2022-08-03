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
use async_lock::RwLock;
use async_std::io::BufReader;
use async_std::net::{TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::task::spawn;

use futures::channel::mpsc::unbounded;
use futures::SinkExt;
use log::{info, trace, warn};

use crate::util::{Receiver, Result, Sender};

pub struct Command {
    tag: String,
    command: String,
    args: Vec<String>,
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

impl ToString for Response {
    fn to_string(&self) -> String {
        format!("{} {} {} {}", self.tag, self.status, self.command, self.message)
    }
}

#[async_trait::async_trait]
pub trait HandleCommand {
    fn name<'a>(&self) -> &'a str;
    async fn validate<'a>(&self, command: &'a Command) -> Result<()>;
    async fn handle<'a>(&self, command: &'a Command) -> Result<Response>;
}

pub struct DelegatingCommandHandler {
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
            command: command.command.clone(),
            message: "Command unknown".to_string(),
        })
    }
}

impl DelegatingCommandHandler {
    pub fn new() -> DelegatingCommandHandler {
        DelegatingCommandHandler {
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    pub async fn register_command(&self, handler: impl HandleCommand + Send + Sync + 'static) {
        let write_lock = &mut *self.handlers.write().await;
        write_lock.push(Box::new(handler));
    }
}

pub struct LoginHandler{}
#[async_trait::async_trait]
impl HandleCommand for LoginHandler {
    fn name<'a>(&self) -> &'a str {
        "LOGIN"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command != self.name() {
            ()
        }
        if command.args.len() < 2 {
            // return error
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Response> {
        // TODO: implement user database lookup
        // TODO: add user to some state management
        let mut user = command.args[0].clone();
        let _password = &command.args[1];
        user = user.replace("\"", "");
        Ok(Response {
            tag: command.tag.clone(),
            status: ResponseStatus::OK,
            command: command.command.clone(),
            message: "completed".to_string(),
        })
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
