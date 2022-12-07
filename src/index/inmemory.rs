use std::{collections::HashMap, error::Error, fmt::Display};

use async_lock::RwLock;

use super::{Index, Mailbox, MailboxError, Permission};

pub struct InMemoryIndex {
    mailboxes: RwLock<HashMap<String, Mailbox>>,
}

impl InMemoryIndex {
    pub fn new() -> Self {
        Self {
            mailboxes: RwLock::new(HashMap::new()),
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
    async fn add_mailbox(&self, mailbox: Mailbox) -> Result<(), MailboxError> {
        let mut write_lock = self.mailboxes.write().await;
        if let Some(..) = write_lock.get(mailbox.name.to_str().unwrap()) {
            return Err(MailboxError::Exists(
                mailbox.name.clone().to_str().unwrap().to_string(),
            ));
        };
        write_lock.insert(
            mailbox
                .name
                .to_str()
                .expect("Cannot convert folder to string")
                .to_string(),
            Mailbox::new(
                &mailbox.name.to_string_lossy(),
                0,
                vec![],
                Permission::ReadOnly,
            ),
        );
        Ok(())
    }
    async fn get_mailbox(
        &self,
        name: &str,
        permission: Permission,
    ) -> Result<Mailbox, MailboxError> {
        let read_lock = self.mailboxes.read().await;
        match read_lock.get(name) {
            Some(mailbox) => Ok(Mailbox {
                permission,
                ..mailbox.clone()
            }),
            None => {
                if "INBOX".eq_ignore_ascii_case(name) {
                    drop(read_lock);
                    self.add_mailbox(Mailbox::new("INBOX", 0, vec![], Permission::ReadOnly)).await?;
                    return Ok(self.mailboxes.read().await.get("INBOX").expect("INBOX has already been inserted so there should be no issue retrieving the inbox from the mailboxes map").clone())
                }
                Err(MailboxError::DoesNotExist(name.to_string()))
        },
        }
    }
}
