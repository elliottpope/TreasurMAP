

pub struct User {
    _authenticated: bool,
    _username: String
}

pub trait Authenticator {
    fn authenticate(&self) -> Option<User>;
}

pub struct BasicAuthenticator {
    username: String,
    password: String
}

impl BasicAuthenticator {
    pub fn new(username: String, password: String) -> BasicAuthenticator {
        return BasicAuthenticator {
            username: username,
            password: password
        }
    }
}

impl Authenticator for BasicAuthenticator {
    fn authenticate(&self) -> Option<User> {
        if self.username.eq(&String::from("test")) && self.password.eq(&String::from("test")) {
            let user = self.username.clone();
            return Some(User {
                _authenticated: true,
                _username: user
            })
        } else {
            return None
        }
    }
}
