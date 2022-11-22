pub mod inmemory;
pub mod error;

use futures::channel::oneshot::{Receiver, Sender};

use bcrypt::{DEFAULT_COST, hash_with_result, BcryptError, verify, Version};
use log::error;

use crate::util::Result;

use self::error::AuthenticationFailed;

#[async_trait::async_trait]
pub trait Authenticate{
    async fn authenticate<T: AuthenticationPrincipal + Send + Sync + 'static>(&mut self, user: T) -> Receiver<Result<User>>;
}
#[async_trait::async_trait]
pub trait UserStore {
    async fn get(&self, username: &str) -> Result<Option<&User>>;
    async fn add(&mut self, user: User) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct User {
    name: String,
    password_hash: Password,
}

impl User {
    pub fn new(username: &str, password: &str) -> Self {
        User { name: username.to_string(), password_hash: Password::new(password).unwrap() }
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Password {
    hash: String,
    _salt: String,
    _cost: u32
}

impl Password {
    pub fn new(password: &str) -> std::result::Result<Self, BcryptError> {
        hash_with_result(password, DEFAULT_COST).map(|hash| Password{
            hash: hash.format_for_version(Version::TwoB),
            _salt: hash.get_salt(),
            _cost: hash.get_cost(),
        })
    }
}

#[async_trait::async_trait]
pub trait AuthenticationPrincipal{
    fn principal(&self) -> String;
    async fn authenticate(&self, user: &User) -> Result<()>;
}

#[derive(Debug)]
pub struct BasicAuth {
    username: String,
    password: String,
}
#[async_trait::async_trait]
impl AuthenticationPrincipal for BasicAuth{
    fn principal(&self) -> String {
        self.username.clone()
    }
    async fn authenticate(&self,user: &User) -> Result<()> {
        match verify(&self.password, &user.password_hash.hash) {
            Ok(success) => {
                if success {
                    return Ok(())
                }
                Err(Box::new(AuthenticationFailed{}))
            },
            Err(e) => {
                error!("password hash verification failed due to {}", e);
                Err(Box::new(AuthenticationFailed{}))
            }
        }
    }
}
impl BasicAuth {
    pub fn from(username: &str, password: &str) -> Self {
        BasicAuth { username: username.to_string(), password: password.to_string() }
    }
}

#[derive(Debug)]
pub struct AuthRequest<T> {
    pub responder: Sender<Result<User>>,
    pub principal: T
}

#[cfg(test)]
mod tests {
    use super::{User, Password, BasicAuth, AuthenticationPrincipal};

    #[async_std::test]
    async fn test_can_authenticate_basic_auth() {
        let user = User{
            name: "me".to_string(),
            password_hash: Password::new("password").unwrap(),
        };
        let auth = BasicAuth::from("me", "password");
        assert!(auth.authenticate(&user).await.is_ok());
    }
    #[async_std::test]
    async fn test_can_fail_authenticate_basic_auth() {
        let user = User{
            name: "me".to_string(),
            password_hash: Password::new("password").unwrap(),
        };
        let auth = BasicAuth::from("me", "password2");
        assert!(auth.authenticate(&user).await.is_err());
    }
}