pub mod fetch;
pub mod login;
pub mod logout;
pub mod select;

use std::sync::Arc;

use async_lock::RwLock;

use crate::connection::Request;
use crate::server::{Command, Response, ResponseStatus};
use crate::util::{Receiver, Result};

#[async_trait::async_trait]
pub trait Handle {
    fn command<'a>(&self) -> &'a str;
    async fn start<'a>(&'a mut self, requests: Receiver<Request>) -> Result<()>;
}

#[async_trait::async_trait]
pub trait HandleCommand {
    fn name<'a>(&self) -> &'a str;
    async fn validate<'a>(&self, command: &'a Command) -> Result<()>;
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>>;
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
            if handler.name() != command.command() {
                continue;
            }
            match handler.validate(&command).await {
                Ok(..) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        let read_lock = &*self.handlers.read().await;
        for handler in read_lock {
            if handler.name() != command.command() {
                continue;
            }
            match handler.handle(&command).await {
                Ok(response) => return Ok(response),
                Err(..) => continue,
            }
        }
        Ok(vec![Response::new(
            &command.tag(),
            ResponseStatus::NO,
            "Command unknown",
        )])
    }
}

impl DelegatingCommandHandler {
    pub fn new() -> DelegatingCommandHandler {
        DelegatingCommandHandler {
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    pub async fn register_command<T: HandleCommand + Send + Sync + 'static>(&self, handler: T) {
        let write_lock = &mut *self.handlers.write().await;
        write_lock.push(Box::new(handler));
    }
}

#[cfg(test)]
pub mod tests {
    use async_std::{stream::StreamExt, task::spawn};
    use futures::{
        channel::mpsc::{self, unbounded, UnboundedReceiver, UnboundedSender},
        SinkExt,
    };

    use crate::{
        connection::{Context, Event, Request},
        server::{Command, Response},
    };

    use super::Handle;

    pub async fn test_handle<T: Handle + Send + Sync + 'static, F: FnOnce(Vec<Response>), S: FnOnce(Event)>(
        mut handler: T,
        command: Command,
        assertions: F,
        event_assertions: S,
        state: Option<Context>,
    ) {
        let (mut requests, requests_receiver): (
            mpsc::UnboundedSender<Request>,
            mpsc::UnboundedReceiver<Request>,
        ) = unbounded();

        let handle = spawn(async move { handler.start(requests_receiver).await });

        let (responder, mut responses): (
            UnboundedSender<Vec<Response>>,
            UnboundedReceiver<Vec<Response>>,
        ) = unbounded();
        let (events, mut event_handler): (UnboundedSender<Event>, UnboundedReceiver<Event>) =
            unbounded();
        let login_request = Request {
            command,
            responder,
            context: state.unwrap_or_default(),
            events,
        };
        requests.send(login_request).await.unwrap();
        if let Some(response) = responses.next().await {
            assertions(response);
        }
        if let Some(event) = event_handler.next().await {
            event_assertions(event);
        }
        drop(requests);
        handle.await.unwrap();
    }
}
