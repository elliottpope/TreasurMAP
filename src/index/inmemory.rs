use std::{collections::HashMap, error::Error, fmt::Display};

use crate::util::Result;

use super::{Index, Mailbox, Permission};

pub struct InMemoryIndex {
    mailboxes: HashMap<String, Mailbox>,
}

impl InMemoryIndex {
    pub fn new() -> Self {
        Self {
            mailboxes: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct MailboxDoesNotExist {
    name: String,
}
impl Error for MailboxDoesNotExist {}
impl Display for MailboxDoesNotExist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mailbox {} does not exist", self.name)
    }
}

#[async_trait::async_trait]
impl Index for InMemoryIndex {
    async fn get_mailbox(&self, name: &str, permission: Permission) -> Result<Mailbox> {
        match self.mailboxes.get(name) {
            Some(mailbox) => Ok(Mailbox { 
                permission,
                ..mailbox.clone()
             }),
            None => Err(Box::new(MailboxDoesNotExist{name: name.clone().to_string()})),
        }
    }
}