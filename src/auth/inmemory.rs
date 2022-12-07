use std::collections::HashMap;
use std::sync::Arc;

use async_std::task::block_on;

use super::error::{UserAlreadyExists, UserStoreError};
use super::{Authenticate, AuthenticationPrincipal, Password, User, UserStore};

use crate::util::Result;

pub struct InMemoryUserStore {
    users: HashMap<String, User>,
}

#[async_trait::async_trait]
impl UserStore for InMemoryUserStore {
    async fn authenticate(&self, principal: Box<dyn AuthenticationPrincipal>) -> Result<User> {
        match self.users.get(&principal.principal()) {
            Some(user) => Ok(user.clone()),
            None => Err(Box::new(UserStoreError::DoesNotExist(
                principal.principal(),
            ))),
        }
    }

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

pub struct InMemoryAuthenticator {
    user_store: Arc<Box<dyn UserStore>>,
}
impl InMemoryAuthenticator {
    pub fn new(user_store: Arc<Box<dyn UserStore>>) -> Self {
        Self { user_store }
    }
}
#[async_trait::async_trait]
impl Authenticate for InMemoryAuthenticator {
    async fn authenticate(&self, user: Box<dyn AuthenticationPrincipal>) -> Result<User> {
        self.user_store.authenticate(user).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::auth::{Authenticate, BasicAuth};

    use super::{InMemoryAuthenticator, InMemoryUserStore};

    #[async_std::test]
    async fn test_can_authenticate() {
        let authenticator = InMemoryAuthenticator::new(Arc::new(Box::new(
            InMemoryUserStore::new().with_user("test@email.com", "password"),
        )));
        let principal = Box::new(BasicAuth::from("test@email.com", "password"));
        if let Ok(result) = authenticator.authenticate(principal).await {
            assert_eq!(result.name, "test@email.com");
        }
    }
}
