use futures::{SinkExt, StreamExt};

use crate::auth::{Authenticate, BasicAuth};
use crate::connection::Request;
use crate::handlers::HandleCommand;
use crate::server::{Command, ParseError, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct LoginHandler<T: Authenticate> {
    authenticator: T,
}
#[async_trait::async_trait]
impl<T: Authenticate + Send + Sync> HandleCommand for LoginHandler<T> {
    fn name<'a>(&self) -> &'a str {
        "LOGIN"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            ()
        }
        if command.num_args() < 2 {
            return Err(Box::new(ParseError {}));
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        // TODO: implement user database lookup
        // TODO: add user to some state management
        let mut _user = command.arg(0);
        let _password = &command.arg(1);
        _user = _user.replace("\"", "");
        Ok(vec![Response::new(
            command.tag(),
            ResponseStatus::OK,
            "LOGIN completed.".to_string(),
        )])
    }
}
impl<T: Authenticate> LoginHandler<T> {
    pub fn new(authenticator: T) -> Self {
        LoginHandler { authenticator }
    }
}
#[async_trait::async_trait]
impl<T: Authenticate + Send + Sync, 'a> Handle for LoginHandler<T> {
    fn command<'b>(&self) -> &'b str {
        "LOGIN"
    }
    async fn start<'b>(&'b mut self, mut requests: Receiver<Request>) -> Result<()> {
        while let Some(mut request) = requests.next().await {
            if let Err(..) = self.validate(&request.command).await {
                request
                    .responder
                    .send(vec![Response::new(
                        request.command.tag(),
                        ResponseStatus::BAD,
                        "insufficient arguments".to_string(),
                    )])
                    .await?;
                continue;
            }
            let mut user = request.command.arg(0);
            let password = &request.command.arg(1);
            user = user.replace("\"", "");
            // TODO: handle password hashing error
            let response = self
                .authenticator
                .authenticate(BasicAuth::from(&user, &password))
                .await;
            match response.await? {
                Ok(result) => {
                    request
                        .responder
                        .send(vec![Response::new(
                            request.command.tag(),
                            ResponseStatus::OK,
                            format!("LOGIN completed. Welcome {}.", &result.name()).to_string(),
                        )])
                        .await?;
                }
                Err(..) => {
                    request
                        .responder
                        .send(vec![Response::new(
                            request.command.tag(),
                            ResponseStatus::BAD,
                            "LOGIN failed.".to_string(),
                        )])
                        .await?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::channel::oneshot::{channel, Receiver, Sender};

    use super::LoginHandler;
    use crate::auth::error::UserDoesNotExist;
    use crate::auth::{Authenticate, AuthenticationPrincipal, User};
    use crate::handlers::tests::test_handle;
    use crate::server::{Command, Response, ResponseStatus};
    use crate::util::Result;

    const EMAIL: &str = "my@email.com";

    struct TestAuthenticator {}
    #[async_trait::async_trait]
    impl Authenticate for TestAuthenticator {
        async fn authenticate<T: AuthenticationPrincipal + Send + Sync>(
            &mut self,
            user: T,
        ) -> Receiver<Result<User>> {
            let (sender, receiver): (Sender<Result<User>>, Receiver<Result<User>>) = channel();
            if user.principal() == EMAIL {
                sender
                    .send(Ok(User::new(&user.principal(), "password")))
                    .unwrap();
            } else {
                sender
                    .send(Err(UserDoesNotExist::new(&user.principal())))
                    .unwrap();
            }
            receiver
        }
    }

    async fn test_login<F: FnOnce(Vec<Response>)>(command: Command, assertions: F) {
        let authenticator = TestAuthenticator {};
        let login_handler = LoginHandler::new(authenticator);

        test_handle(login_handler, command, assertions).await;
        
    }

    #[async_std::test]
    async fn test_can_login() {
        let login_command = Command::new(
            "a1",
            "LOGIN",
            vec![EMAIL, "password"],
        );
        test_login(login_command, login_success).await;
    }

    #[async_std::test]
    async fn test_login_success_command_lower_case() {
        let login_command = Command::new(
            "a1",
            "login",
            vec![EMAIL, "password"],
        );
        test_login(login_command, login_success).await;
    }

    #[async_std::test]
    async fn test_login_success_command_camel_case() {
        let login_command = Command::new(
            "a1",
            "Login",
            vec![EMAIL, "password"],
        );
        test_login(login_command, login_success).await;
    }

    fn login_success(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new(
                "a1".to_string(),
                ResponseStatus::OK,
                "LOGIN completed. Welcome my@email.com.".to_string()
            )
        );
    }

    #[async_std::test]
    async fn test_login_bad_user() {
        let login_command = Command::new(
            "a1",
            "LOGIN",
            vec!["not.a.user@domain.com", "password"],
        );
        test_login(login_command, login_failed).await;
    }

    #[async_std::test]
    async fn test_login_insufficient_args() {
        let login_command = Command::new(
            "a1",
            "LOGIN",
            vec![EMAIL],
        );
        test_login(login_command, |response| {
            assert_eq!(response.len(), 1 as usize);
            let reply = &response[0];
            assert_eq!(
                reply,
                &Response::new(
                    "a1".to_string(),
                    ResponseStatus::BAD,
                    "insufficient arguments".to_string()
                )
            );
        })
        .await;
    }

    fn login_failed(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new(
                "a1".to_string(),
                ResponseStatus::BAD,
                "LOGIN failed.".to_string()
            )
        );
    }
}
