use std::collections::HashMap;
use std::sync::Arc;

use async_std::task::block_on;
use futures::{SinkExt, StreamExt};
use futures::channel::oneshot::{Sender, Receiver, channel};
use futures::channel::mpsc::{self, UnboundedReceiver};

use super::{UserStore, User, UserAlreadyExists, Authenticate, AuthRequest, UserDoesNotExist};

use crate::util::Result;

pub struct InMemoryUserStore {
    users: HashMap<String, User>,
}
pub struct InMemoryAuthenticator {
    pub requests: mpsc::UnboundedSender<AuthRequest>,
}

#[async_trait::async_trait]
impl UserStore for InMemoryUserStore {
    async fn get(&self, username: &str) ->  Result<Option<&User>> {
        Ok(self.users.get(username))
    }

    async fn add(&mut self,user: User) -> Result<()> {
        let username = user.name.clone();
        if self.users.contains_key(&username) {
            return Err(UserAlreadyExists::new(&username))
        }
        match self.users.insert(username.clone(), user) {
            Some(..) => Err(UserAlreadyExists::new(&username)),
            None => Ok(())
        }
    }
}

impl InMemoryUserStore {
    pub fn new() -> Self {
        InMemoryUserStore { users: HashMap::new() }
    }
    pub fn with_user(mut self, username: &str, _password: &str) -> Self {
        block_on(self.add(User{name: username.to_string()})).unwrap();
        self
    }
}

#[async_trait::async_trait]
impl Authenticate for InMemoryAuthenticator {
    async fn authenticate(&mut self,user:User) -> Receiver<Result<User>> {
        let (sender, receiver): (Sender<Result<User>>, Receiver<Result<User>>) = channel();
        // place task on internal queue
        self.requests.send(AuthRequest { responder: sender, user }).await.unwrap();
        receiver
    }
}

impl InMemoryAuthenticator {
    pub async fn start(user_store: Arc<Box<dyn UserStore + Send + Sync>>, mut requests: UnboundedReceiver<AuthRequest>) -> Result<()> {
        while let Some(request) = requests.next().await {
            match user_store.get(&request.user.name).await {
                Ok(option) => match option {
                    Some(user) => request.responder.send(Ok(user.clone())).unwrap(),
                    None => request.responder.send(Err(UserDoesNotExist::new(&request.user.name))).unwrap(),
                },
                Err(e) => request.responder.send(Err(e)).unwrap(),
            };
        };
        Ok(())
    }
}