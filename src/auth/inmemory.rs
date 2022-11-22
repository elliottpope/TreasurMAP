use std::collections::HashMap;
use std::sync::Arc;

use async_std::task::block_on;
use futures::channel::mpsc::{self, UnboundedReceiver};
use futures::channel::oneshot::{channel, Receiver, Sender};
use futures::{SinkExt, StreamExt};

use super::error::{UserAlreadyExists, UserDoesNotExist};
use super::{AuthRequest, Authenticate, AuthenticationPrincipal, Password, User, UserStore};

use crate::util::Result;

pub struct InMemoryUserStore {
    users: HashMap<String, User>,
}
pub struct InMemoryAuthenticator {
    pub requests:
        mpsc::UnboundedSender<AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>>,
}

#[async_trait::async_trait]
impl UserStore for InMemoryUserStore {
    async fn get(&self, username: &str) -> Result<Option<&User>> {
        Ok(self.users.get(username))
    }

    async fn add(&mut self, user: User) -> Result<()> {
        let username = user.name.clone();
        if self.users.contains_key(&username) {
            return Err(UserAlreadyExists::new(&username));
        }
        match self.users.insert(username.clone(), user) {
            Some(..) => Err(UserAlreadyExists::new(&username)),
            None => Ok(()),
        }
    }
}

impl InMemoryUserStore {
    pub fn new() -> Self {
        InMemoryUserStore {
            users: HashMap::new(),
        }
    }
    pub fn with_user(mut self, username: &str, password: &str) -> Self {
        block_on(self.add(User {
            name: username.to_string(),
            password_hash: Password::new(password).unwrap(),
        }))
        .unwrap();
        self
    }
}

#[async_trait::async_trait]
impl Authenticate for InMemoryAuthenticator {
    async fn authenticate<T: AuthenticationPrincipal + Send + Sync + 'static>(
        &mut self,
        principal: T,
    ) -> Receiver<Result<User>> {
        let (sender, receiver): (Sender<Result<User>>, Receiver<Result<User>>) = channel();
        // place task on internal queue
        self.requests
            .send(AuthRequest {
                responder: sender,
                principal: Box::new(principal),
            })
            .await
            .unwrap();
        receiver
    }
}

impl InMemoryAuthenticator {
    pub async fn start(
        user_store: Arc<Box<dyn UserStore + Send + Sync>>,
        mut requests: UnboundedReceiver<
            AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>,
        >,
    ) -> Result<()> {
        while let Some(request) = requests.next().await {
            match user_store.get(&request.principal.principal()).await {
                Ok(option) => match option {
                    Some(user) => match request.principal.authenticate(user).await {
                        Ok(..) => request.responder.send(Ok(user.clone())).unwrap(),
                        Err(e) => request.responder.send(Err(e)).unwrap(),
                    },
                    None => request
                        .responder
                        .send(Err(UserDoesNotExist::new(&request.principal.principal())))
                        .unwrap(),
                },
                Err(e) => request.responder.send(Err(e)).unwrap(),
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_std::task::spawn;
    use futures::{channel::{mpsc::{unbounded, UnboundedReceiver, UnboundedSender}, oneshot::{Sender, Receiver, channel}}, SinkExt};

    use crate::{
        auth::{AuthRequest, AuthenticationPrincipal, User, BasicAuth},
        util::Result,
    };

    use super::{InMemoryAuthenticator, InMemoryUserStore};

    #[async_std::test]
    async fn test_can_authenticate() {
        let (mut sender, receiver): (
            UnboundedSender<AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>>,
            UnboundedReceiver<AuthRequest<Box<dyn AuthenticationPrincipal + Send + Sync>>>,
        ) = unbounded();
        let handle = spawn(
            InMemoryAuthenticator::start(Arc::new(Box::new(InMemoryUserStore::new().with_user("test@email.com", "password"))), receiver)
        );
        let (request, response): (Sender<Result<User>>, Receiver<Result<User>>) = channel();
        sender.send(AuthRequest { responder: request, principal: Box::new(BasicAuth::from("test@email.com", "password")) }).await.unwrap();
        if let Ok(result) = response.await {
            assert!(result.is_ok());
            let user = result.unwrap();
            assert_eq!(user.name, "test@email.com");
        }

        drop(sender);
        handle.await.unwrap();
    }
}
