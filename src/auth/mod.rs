pub mod inmemory;

use std::{error::Error, fmt::{Display, Formatter}, fmt};

use futures::channel::oneshot::{Receiver, Sender};

use crate::util::Result;

#[async_trait::async_trait]
pub trait Authenticate{
    // TODO: use more general AuthenticationData rather than User
    async fn authenticate(&mut self, user: User) -> Receiver<Result<User>>;
}
#[async_trait::async_trait]
pub trait UserStore {
    async fn get(&self, username: &str) -> Result<Option<&User>>;
    async fn add(&mut self, user: User) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
}

#[derive(Debug)]
pub struct UserAlreadyExists {
    username: String
}
#[derive(Debug)]
pub struct UserDoesNotExist {
    username: String
}

impl Error for UserAlreadyExists {}
impl Display for UserAlreadyExists {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "user {} already exists", self.username)
    }
}
impl UserAlreadyExists {
    pub fn new(username: &str) -> Box<Self> {
        Box::new(UserAlreadyExists { username: username.to_string() })
    }
}

impl Error for UserDoesNotExist {}
impl Display for UserDoesNotExist {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "user {} does not exist", self.username)
    }
}
impl UserDoesNotExist {
    pub fn new(username: &str) -> Box<Self> {
        Box::new(UserDoesNotExist { username: username.to_string() })
    }
}

#[derive(Debug)]
pub struct AuthRequest {
    pub responder: Sender<Result<User>>,
    pub user: User
}
