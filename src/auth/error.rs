use std::{fmt::{Display, Formatter, self}, error::Error};



#[derive(Debug)]
pub struct UserAlreadyExists {
    username: String
}
#[derive(Debug)]
pub struct UserDoesNotExist {
    username: String
}
#[derive(Debug)]
pub struct AuthenticationFailed {
}

impl Error for AuthenticationFailed{}
impl Display for AuthenticationFailed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "authentication failed")
    }
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