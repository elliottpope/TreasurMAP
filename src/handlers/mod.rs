pub mod login;
pub mod select;

use std::sync::Arc;

use async_lock::RwLock;

use crate::util::{Result};
use crate::server::{Command, Response, ResponseStatus};

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
        Ok(Response::new(
            command.tag(),
            ResponseStatus::NO,
            command.command(),
            "Command unknown".to_string(),
        ))
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
