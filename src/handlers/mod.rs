pub mod login;
pub mod select;
pub mod fetch;
pub mod logout;

use std::sync::Arc;

use async_lock::RwLock;

use crate::util::{Result};
use crate::server::{Command, Response, ResponseStatus};

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
        Ok(vec!(Response::new(
            command.tag(),
            ResponseStatus::NO,
            "Command unknown".to_string(),
        )))
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
