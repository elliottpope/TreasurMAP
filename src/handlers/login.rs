use std::sync::Arc;

use futures::{SinkExt, StreamExt};

use crate::auth::{Authenticate, BasicAuth};
use crate::connection::{Event, Request};
use crate::handlers::HandleCommand;
use crate::server::{Command, ParseError, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct LoginHandler {
    authenticator: Arc<Box<dyn Authenticate>>,
}
#[async_trait::async_trait]
impl HandleCommand for LoginHandler {
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
            &command.tag(),
            ResponseStatus::OK,
            "LOGIN completed.",
        )])
    }
}
impl LoginHandler {
    pub fn new(authenticator: Arc<Box<dyn Authenticate>>) -> Self {
        LoginHandler { authenticator }
    }
}
#[async_trait::async_trait]
impl<'a> Handle for LoginHandler {
    fn command<'b>(&self) -> &'b str {
        "LOGIN"
    }
    async fn start<'b>(&'b mut self, mut requests: Receiver<Request>) -> Result<()> {
        while let Some(mut request) = requests.next().await {
            if let Err(..) = self.validate(&request.command).await {
                request
                    .responder
                    .send(vec![Response::new(
                        &request.command.tag(),
                        ResponseStatus::BAD,
                        "insufficient arguments",
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
                .authenticate(Box::new(BasicAuth::from(&user, &password)))
                .await;
            match response {
                Ok(result) => {
                    let message = format!("LOGIN completed. Welcome {}.", &result.name());
                    request.events.send(Event::AUTH(result)).await?;
                    request
                        .responder
                        .send(vec![Response::new(
                            &request.command.tag(),
                            ResponseStatus::OK,
                            &message,
                        )])
                        .await?;
                }
                Err(..) => {
                    request
                        .responder
                        .send(vec![Response::new(
                            &request.command.tag(),
                            ResponseStatus::BAD,
                            "LOGIN failed.",
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
    use std::sync::Arc;

    use super::LoginHandler;
    use crate::auth::error::UserDoesNotExist;
    use crate::auth::{Authenticate, AuthenticationPrincipal, User};
    use crate::connection::Event;
    use crate::handlers::tests::test_handle;
    use crate::server::{Command, Response, ResponseStatus};
    use crate::util::Result;

    const EMAIL: &str = "my@email.com";

    struct TestAuthenticator {}
    #[async_trait::async_trait]
    impl Authenticate for TestAuthenticator {
        async fn authenticate(&self, user: Box<dyn AuthenticationPrincipal>) -> Result<User> {
            if user.principal() == EMAIL {
                return Ok(User::new(&user.principal(), "password"));
            }
            return Err(UserDoesNotExist::new(&user.principal()));
        }
    }

    async fn test_login<F: FnOnce(Vec<Response>)>(
        command: Command,
        assertions: F,
        should_auth: bool,
    ) {
        let authenticator: Arc<Box<dyn Authenticate>> = Arc::new(Box::new(TestAuthenticator {}));
        let login_handler = LoginHandler::new(authenticator);

        let mut event_assertions = Some(|event| match event {
            Event::AUTH(user) => {
                assert_eq!(user.name(), EMAIL);
            }
            _ => {
                panic!("LoginHandler should only send AUTH events")
            }
        });
        if !should_auth {
            event_assertions.take();
        }
        test_handle(login_handler, command, assertions, event_assertions, None).await;
    }

    #[async_std::test]
    async fn test_can_login() {
        let login_command = Command::new("a1", "LOGIN", vec![EMAIL, "password"]);
        test_login(login_command, login_success, true).await;
    }

    #[async_std::test]
    async fn test_login_success_command_lower_case() {
        let login_command = Command::new("a1", "login", vec![EMAIL, "password"]);
        test_login(login_command, login_success, true).await;
    }

    #[async_std::test]
    async fn test_login_success_command_camel_case() {
        let login_command = Command::new("a1", "Login", vec![EMAIL, "password"]);
        test_login(login_command, login_success, true).await;
    }

    fn login_success(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new(
                "a1",
                ResponseStatus::OK,
                "LOGIN completed. Welcome my@email.com."
            )
        );
    }

    #[async_std::test]
    async fn test_login_bad_user() {
        let login_command = Command::new("a1", "LOGIN", vec!["not.a.user@domain.com", "password"]);
        test_login(login_command, login_failed, false).await;
    }

    #[async_std::test]
    async fn test_login_insufficient_args() {
        let login_command = Command::new("a1", "LOGIN", vec![EMAIL]);
        test_login(
            login_command,
            |response| {
                assert_eq!(response.len(), 1 as usize);
                let reply = &response[0];
                assert_eq!(
                    reply,
                    &Response::new("a1", ResponseStatus::BAD, "insufficient arguments")
                );
            },
            false,
        )
        .await;
    }

    fn login_failed(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new("a1", ResponseStatus::BAD, "LOGIN failed.")
        );
    }
}
