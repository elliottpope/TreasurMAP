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

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::mpsc::{self, channel};
use std::sync::Arc;
use std::time::Duration;

use async_listen::{error_hint, ListenExt};
use async_std::net::TcpListener;
use async_std::prelude::*;
use async_std::task::{spawn, JoinHandle};

use futures::channel::mpsc::unbounded;
use log::{trace, warn};

use crate::auth::inmemory::{InMemoryAuthenticator, InMemoryUserStore};
use crate::auth::{AuthRequest, UserStore, Authenticate, AuthenticationPrincipal};
use crate::connection::{Connection, Request};
use crate::handlers::fetch::FetchHandler;
use crate::handlers::login::LoginHandler;
use crate::handlers::Handle;
use crate::handlers::select::SelectHandler;
use crate::util::{Receiver, Result, Sender};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Command {
    tag: String,
    command: String,
    args: Vec<String>,
}

impl Command {
    pub fn new(tag: &str, command: &str, args: Vec<&str>) -> Command {
        Command {
            tag: tag.to_string(),
            command: command.to_uppercase(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }
    pub fn tag(&self) -> String {
        self.tag.clone()
    }
    pub fn command(&self) -> String {
        self.command.clone()
    }
    pub fn arg(&self, position: usize) -> String {
        if self.args.len() <= position {
            return "".to_string();
        }
        self.args[position].clone()
    }
    pub fn num_args(&self) -> usize {
        self.args.len()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
impl ResponseStatus {
    pub fn from(string: String) -> std::result::Result<ResponseStatus, ParseError> {
        match string.as_str() {
            "OK" => Ok(ResponseStatus::OK),
            "BAD" => Ok(ResponseStatus::BAD),
            "NO" => Ok(ResponseStatus::NO),
            &_ => Err(ParseError {}),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Response {
    tag: String,
    status: Option<ResponseStatus>,
    message: String,
}

impl Response {
    pub fn new(tag: String, status: ResponseStatus, message: String) -> Response {
        Response {
            tag,
            status: Some(status),
            message,
        }
    }
    pub fn from(string: &str) -> std::result::Result<Response, ParseError> {
        let components: Vec<String> = string.split(" ").map(|s| s.to_string()).collect();
        if components.len() < 3 {
            return Err(ParseError {});
        }
        let status = match ResponseStatus::from(components[1].clone()) {
            Ok(status) => Some(status),
            Err(_) => None,
        };
        Ok(Response {
            tag: components[0].clone(),
            status,
            message: match status {
                Some(_) => components[2..].join(" "),
                None => components[1..].join(" "),
            },
        })
    }
    pub fn tag(&self) -> String {
        self.tag.clone()
    }
    pub fn status(&self) -> Option<ResponseStatus> {
        self.status
    }
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl ToString for Response {
    fn to_string(&self) -> String {
        match self.status {
            Some(status) => format!("{} {} {}", self.tag, status, self.message),
            None => format!("{} {}", self.tag, self.message),
        }
    }
}

#[derive(Debug)]
pub struct ParseError;
impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "ParseError")
    }
}
impl Error for ParseError {}

impl Command {
    pub fn parse(cmd: &str) -> std::result::Result<Command, ParseError> {
        let mut values: VecDeque<String> = cmd.split(" ").map(|s| s.to_string()).collect();
        let tag = match values.pop_front() {
            Some(t) => t,
            None => return Err(ParseError {}),
        };
        let command = match values.pop_front() {
            Some(c) => c,
            None => return Err(ParseError {}),
        };
        values = values.iter().map(|arg| {
            let length = arg.len();
            if arg.starts_with("\"") && arg.ends_with("\"") || arg.starts_with("'") && arg.ends_with("'") {
                return arg[1..length-1].to_string()
            }
            return arg.to_string()
        }).collect();
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

pub struct Server {
    config: Configuration,
    handler: HashMap<String, Sender<Request>>,
    user_store: Arc<Box<dyn UserStore + Send + Sync>>,
}

impl Default for Server {
    fn default() -> Server {
        let config = Configuration::default(); // TODO: add the new, from_env, and from_file options to override configs
        let handler: HashMap<String, Sender<Request>> = HashMap::new();
        Server::new(config, handler)
    }
}

impl Server {
    pub async fn start(&mut self) -> Result<()> {
        let (sender, _receiver) = channel();
        self.start_with_notification(sender).await
    }

    pub fn new(config: Configuration, handler: HashMap<String, Sender<Request>>) -> Self {
        Server { 
            config, 
            handler, 
            user_store: Arc::new(Box::new(InMemoryUserStore::new())), 
        }
    }

    pub fn with_user_store<T:UserStore + Send + Sync + 'static>(mut self, user_store: T) -> Self {
        self.user_store = Arc::new(Box::new(user_store));
        self
    }

    pub async fn start_with_notification(&mut self, sender: mpsc::Sender<()>) -> Result<()> {
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
        
        let (authenticator, auth_loop) = self.init_auth();
        let mut handlers: Vec<Box<dyn Handle + Send + Sync>> = Vec::new();
        let login = LoginHandler::new(authenticator);

        handlers.push(Box::new(login));
        handlers.push(Box::new(SelectHandler{}));
        handlers.push(Box::new(FetchHandler{}));

        let handlers = self.start_handlers(handlers);

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
                let mut connection = Connection::new(socket).await?;
                connection.handle(&handler).await
            });
        }
        for handler in handlers {
            handler.await?;
        }
        auth_loop.await?;

        Ok(())
    }

    fn start_handlers(&mut self, handlers: Vec<Box<dyn Handle + Send + Sync>>) -> Vec<JoinHandle<Result<()>>> {
        handlers.into_iter().map(|mut handler| {
            let (requests, receiver): (Sender<Request>, Receiver<Request>) = unbounded();
            let cmd = handler.command();
            self.handler.insert(cmd.to_string(), requests);
            spawn(async move { handler.start(receiver).await })
        }).collect()
    }

    fn init_auth(&self) -> (impl Authenticate, JoinHandle<Result<()>>) {
        let user_store = self.user_store.clone();

        let (auth_requests, auth_requests_receiver): (Sender<AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>>, Receiver<AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>>) =
            unbounded();
        let authenticator = InMemoryAuthenticator {
            requests: auth_requests,
        };
        let handle = spawn(
            async move { InMemoryAuthenticator::start(user_store, auth_requests_receiver).await },
        );
        return (authenticator, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn test_can_strip_quotes_from_command() {
        let cmd = Command::parse("a1 LOGIN 'me@email.com' \"password\"");
        assert!(cmd.is_ok());
        assert_eq!(cmd.unwrap(), Command::new("a1", "LOGIN", vec!["me@email.com", "password"]));
    }
}