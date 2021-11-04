use std::collections::HashMap;
use std::result::Result;
use std::clone::Clone;
use std::str::from_utf8;

pub struct User {
    _authenticated: bool,
    _username: String,
}

pub trait Auth {}

pub trait Authenticator<A: Auth> {
    fn authenticate(&self, auth: A) -> Option<User>;
}

pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

impl BasicAuth {
    pub fn from(username: &[u8], password: &[u8]) -> BasicAuth {
        BasicAuth {
            username: String::from(from_utf8(username).unwrap_or_default()),
            password: String::from(from_utf8(password).unwrap_or_default()),
        }
    }
}

impl Auth for BasicAuth {}

pub struct BasicAuthenticator {
    _store: Box<dyn UserStore>,
}

impl BasicAuthenticator {
    pub fn new<S: UserStore + 'static>(store: S) -> BasicAuthenticator {
        return BasicAuthenticator {
            _store: Box::new(store),
        };
    }
}

impl Authenticator<BasicAuth> for BasicAuthenticator {
    fn authenticate(&self, auth: BasicAuth) -> Option<User> {
        return self._store.authenticate(auth.username, auth.password);
    }
}

pub trait UserStore: Send + Sync {
    fn authenticate(&self, username: String, password: String) -> Option<User>;
    fn delete(&mut self, username: String) -> Result<(), ()>;
}

pub struct InMemoryUserStore {
    _users: HashMap<String, String>,
}

impl InMemoryUserStore {
    pub fn new() -> InMemoryUserStore {
        return InMemoryUserStore {
            _users: HashMap::new(),
        };
    }

    pub fn with_user(&mut self, username: String, password: String) -> InMemoryUserStore {
        self._users.insert(username, password);
        self.clone()
    }
}

impl Clone for InMemoryUserStore {
    fn clone(&self) -> InMemoryUserStore {
        InMemoryUserStore {
            _users: self._users.clone()
        }
    }
}

impl UserStore for InMemoryUserStore {
    fn authenticate(&self, username: String, password: String) -> Option<User> {
        match self._users.get(&username) {
            Some(pwd) => {
                if pwd.eq(&password) {
                    return Some(User {
                        _authenticated: true,
                        _username: username,
                    });
                }
                return None;
            }
            None => None,
        }
    }
    fn delete(&mut self, username: String) -> Result<(), ()> {
        match self._users.remove(&username) {
            Some(_) => Ok(()),
            None => Err(())
        }
    }
}
