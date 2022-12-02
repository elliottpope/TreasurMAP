pub mod inmemory;

use std::{error::Error, fmt::Display};

use async_std::path::PathBuf;
use futures::{channel::{mpsc::UnboundedReceiver, oneshot::Sender}, StreamExt};
use log::warn;

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

#[derive(Debug)]
pub enum MailboxError {
    Exists(String),
    DoesNotExist(String),
    InsufficientPermissions(String, String, String),
}
impl Error for MailboxError {}
impl Display for MailboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MailboxError::Exists(name) => {
                write!(f, "Mailbox {} already exists", name)
            },
            MailboxError::DoesNotExist(name) => {
                write!(f, "Mailbox {} does not exist", name)
            },
            MailboxError::InsufficientPermissions(name, username, requested) => {
                write!(f, "User {} does not have sufficient permissions to {} on mailbox {}", username, requested, name)
            }
        }
    }
}

#[async_trait::async_trait]
pub trait Index: Sync + Send {
    async fn add_mailbox(&mut self, mailbox: Mailbox) -> Result<(), MailboxError>;
    async fn get_mailbox(&self, name: &str, permission: Permission) -> Result<Mailbox, MailboxError>;
    async fn start(&self, mut requests: UnboundedReceiver<GetMailboxRequest>) -> crate::util::Result<()> {
        while let Some(request) = requests.next().await {
            match self.get_mailbox(&request.name, request.permission).await {
                Ok(mailbox) => {
                    request.responder.send(Some(mailbox)).unwrap();
                },
                Err(e) => {
                    // TODO: send MailboxError
                    warn!("{}", e);
                    request.responder.send(None).unwrap();
                },
            }
            
        }
        Ok(())
    }
}