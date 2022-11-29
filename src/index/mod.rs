pub mod inmemory;

use async_std::path::PathBuf;
use futures::{channel::{mpsc::UnboundedReceiver, oneshot::Sender}, StreamExt};
use log::warn;

use crate::util::Result;

#[derive(Debug, Clone, Copy)]
pub enum Permission {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone)]
pub struct Mailbox {
    pub name: PathBuf,
    pub count: u64,
    pub flags: Vec<Flag>,
    pub permission: Permission,
}

#[derive(Debug, Clone)]
pub struct Flag {
    pub value: String,
    pub permanent: bool,
}

#[derive(Debug)]
pub struct GetMailboxRequest {
    pub name: String,
    pub responder: Sender<Option<Mailbox>>,
    pub permission: Permission,
}

impl Mailbox {
    pub fn new(name: &str, count: u64, flags: Vec<Flag>, permission: Permission) -> Self {
        Self {
            name: PathBuf::from(name),
            count,
            flags,
            permission,
        }
    }
}

#[async_trait::async_trait]
pub trait Index {
    async fn get_mailbox(&self, name: &str, permission: Permission) -> Result<Mailbox>;
    async fn start(&self, mut requests: UnboundedReceiver<GetMailboxRequest>) -> Result<()> {
        while let Some(request) = requests.next().await {
            match self.get_mailbox(&request.name, request.permission).await {
                Ok(mailbox) => {
                    request.responder.send(Some(mailbox)).unwrap();
                },
                Err(..) => {
                    warn!("Mailbox {} was requested but does not exist", &request.name);
                    request.responder.send(None).unwrap();
                },
            }
            
        }
        Ok(())
    }
}