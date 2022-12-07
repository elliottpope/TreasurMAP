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

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use async_listen::{error_hint, ListenExt};
use async_std::net::TcpListener;
use async_std::prelude::*;
use async_std::task::{spawn, JoinHandle};
use futures::channel::mpsc::unbounded;
use futures::future::join_all;
use log::{info, trace, warn};

use crate::auth::inmemory::{InMemoryUserStore, InMemoryAuthenticator};
use crate::auth::{UserStore, Authenticate};
use crate::connection::{Connection, Request};
use crate::handlers::Handle;
use crate::handlers::fetch::FetchHandler;
use crate::handlers::login::LoginHandler;
use crate::handlers::logout::LogoutHandler;
use crate::handlers::select::SelectHandler;
use crate::index::inmemory::InMemoryIndex;
use crate::index::Index;
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
    pub fn new(tag: &str, status: ResponseStatus, message: &str) -> Response {
        Response {
            tag: tag.to_string(),
            status: Some(status),
            message: message.to_string(),
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
        values = values
            .iter()
            .map(|arg| {
                let length = arg.len();
                if arg.starts_with("\"") && arg.ends_with("\"")
                    || arg.starts_with("'") && arg.ends_with("'")
                {
                    return arg[1..length - 1].to_string();
                }
                return arg.to_string();
            })
            .collect();
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
    listener: TcpListener,
    handler: Arc<HashMap<String, Sender<Request>>>,
    _user_store: Arc<Box<dyn UserStore>>,
    _index: Arc<Box<dyn Index>>,
    handler_tasks: Vec<JoinHandle<Result<()>>>,
}

impl Server {
    pub async fn listen(self) -> Result<()> {
        trace!("Server starting on {}", &self.config.server.address);
        let mut incoming = self
            .listener
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
        info!(
            "Server started listening on {}",
            &self.config.server.address
        );

        let mut connections = vec![];
        while let Some((token, socket)) = incoming.next().await {
            trace!("New connection from {}", &socket.peer_addr()?);
            let handler = self.handler.clone();
            connections.push(spawn(async move {
                let _holder = token;
                trace!(
                    "Spawning handler for new connection from {}",
                    &socket.peer_addr()?
                );
                let connection = Connection::new(socket).await?;
                connection.handle(handler).await
            }));
        }
        join_all(connections).await;
        join_all(self.handler_tasks).await;
        Ok(())
    }
}

pub struct ServerBuilder {
    user_store: Option<Box<dyn UserStore>>,
    // TODO: replace with DataStore trait
    data_store: Option<Box<dyn Any>>,
    index: Option<Box<dyn Index>>,
    // TODO: replace with Middleware trait
    middleware: Vec<Box<dyn Any>>,
    handlers: HashMap<String, Box<dyn Handle>>,
    authenticator: Option<Box<dyn Authenticate>>,
    configuration: Option<Configuration>,
}

impl ServerBuilder {
    #[must_use]
    pub fn new() -> Self {
        ServerBuilder {
            user_store: None,
            data_store: None,
            index: None,
            middleware: vec![],
            handlers: HashMap::new(),
            authenticator: None,
            configuration: None,
        }
    }
    pub fn with_user_store<U: UserStore + 'static>(mut self, user_store: U) -> Self {
        self.user_store.replace(Box::new(user_store));
        self
    }
    // TODO: replace with DataStore trait
    pub fn with_data_store<D: Any>(mut self, data_store: D) -> Self {
        self.data_store.replace(Box::new(data_store));
        self
    }
    pub fn with_index<I: Index + 'static>(mut self, index: I) -> Self {
        self.index.replace(Box::new(index));
        self
    }
    pub fn with_authenticator<A: Authenticate + 'static>(mut self, authenticator: A) -> Self {
        self.authenticator.replace(Box::new(authenticator));
        self
    }
    // TODO: replace with Middleware trait
    pub fn with_middleware<M: Any>(mut self, middleware: M) -> Self {
        self.middleware.push(Box::new(middleware));
        self
    }
    pub fn with_handler<H: Handle + 'static>(mut self, handler: H) -> Self {
        self.handlers
            .insert(handler.command().clone().to_string(), Box::new(handler));
        self
    }
    pub fn with_configuration(mut self, configuration: Configuration) -> Self {
        self.configuration.replace(configuration);
        self
    }
    pub async fn bind(mut self) -> Result<Server> {
        let configuration = self.configuration.unwrap_or_else(Configuration::default);
        let listener = TcpListener::bind(&configuration.server.address).await?;
        
        let user_store = Arc::new(self.user_store
                    .unwrap_or_else(|| Box::new(InMemoryUserStore::new())));
        let index = Arc::new(self.index.unwrap_or_else(|| Box::new(InMemoryIndex::new())));
        let authenticator = Arc::new(self.authenticator.unwrap_or_else(|| Box::new(InMemoryAuthenticator::new(user_store.clone()))));
        
        // TODO: add default Handlers for IMAPv2rev4 spec (i.e. Login, Select, Fetch, Logout, etc.)
        let select = Box::new(SelectHandler::new(index.clone()));
        let login: Box<dyn Handle> = Box::new(LoginHandler::new(authenticator));
        let fetch = Box::new(FetchHandler{});
        let logout = Box::new(LogoutHandler{});
        self.handlers.insert("LOGIN".to_string(), login);
        self.handlers.insert("SELECT".to_string(), select);
        self.handlers.insert("FETCH".to_string(), fetch);
        self.handlers.insert("LOGOUT".to_string(), logout);
        
        let mut handler_tasks = vec![];
        let handlers: HashMap<String, Sender<Request>> = self
            .handlers
            .drain()
            .map(|(key, mut handler)| {
                let (sender, requests): (Sender<Request>, Receiver<Request>) = unbounded();
                handler_tasks.push(spawn(async move { handler.start(requests).await }));
                (key, sender)
            })
            .collect();

        Ok(Server {
            config: configuration,
            listener,
            handler: Arc::new(handlers),
            handler_tasks,
            _user_store: user_store,
            _index: index,
        })
    }
    pub async fn listen(self) -> Result<()> {
        let server = self.bind().await?;
        server.listen().await
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn test_can_strip_quotes_from_command() {
        let cmd = Command::parse("a1 LOGIN 'me@email.com' \"password\"");
        assert!(cmd.is_ok());
        assert_eq!(
            cmd.unwrap(),
            Command::new("a1", "LOGIN", vec!["me@email.com", "password"])
        )
    }
}
