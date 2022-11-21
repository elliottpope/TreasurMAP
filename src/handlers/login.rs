use futures::{SinkExt, StreamExt};

use crate::auth::{Authenticate, User};
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
            }
            let mut user = request.command.arg(0);
            let _password = &request.command.arg(1);
            user = user.replace("\"", "");
            let response = self.authenticator.authenticate(User { name: user }).await;
            match response.await? {
                Ok(result) => {
                    request
                        .responder
                        .send(vec![Response::new(
                            request.command.tag(),
                            ResponseStatus::OK,
                            format!("LOGIN completed. Welcome {}.", &result.name).to_string(),
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
    use async_std::stream::StreamExt;
    use async_std::task::spawn;
    use futures::channel::mpsc::{self, unbounded, UnboundedReceiver, UnboundedSender};
    use futures::channel::oneshot::{channel, Receiver, Sender};
    use futures::SinkExt;

    use super::LoginHandler;
    use crate::auth::{Authenticate, User, UserDoesNotExist};
    use crate::connection::{Context, Request};
    use crate::handlers::{Handle};
    use crate::server::{Command, Response, ResponseStatus};
    use crate::util::Result;

    struct TestAuthenticator {}
    #[async_trait::async_trait]
    impl Authenticate for TestAuthenticator {
        async fn authenticate(&mut self, user: User) -> Receiver<Result<User>> {
            let (sender, receiver): (Sender<Result<User>>, Receiver<Result<User>>) = channel();
            if user.name == "my@email.com" {
                sender.send(Ok(user)).unwrap();
            } else {
                sender.send(Err(UserDoesNotExist::new(&user.name))).unwrap();
            }
            receiver
        }
    }

    async fn test_login<F: FnOnce(Vec<Response>)>(command: Command, assertions: F) {
        let authenticator = TestAuthenticator {};

        let (mut requests, requests_receiver): (
            mpsc::UnboundedSender<Request>,
            mpsc::UnboundedReceiver<Request>,
        ) = unbounded();

        let mut login_handler = LoginHandler::new(authenticator);
        let handle = spawn(async move { login_handler.start(requests_receiver).await });

        let (responder, mut responses): (
            UnboundedSender<Vec<Response>>,
            UnboundedReceiver<Vec<Response>>,
        ) = unbounded();
        let login_request = Request {
            command,
            responder,
            context: Context {},
        };
        requests.send(login_request).await.unwrap();
        if let Some(response) = responses.next().await {
            assertions(response);
        }
        drop(requests);
        handle.await.unwrap();
    }

    #[async_std::test]
    async fn test_can_login() {
        let login_command = Command::new(
            "a1".to_string(),
            "LOGIN".to_string(),
            vec!["my@email.com".to_string(), "password".to_string()],
        );
        test_login(login_command, login_success).await;
    }

    #[async_std::test]
    async fn test_login_success_command_lower_case() {
        let login_command = Command::new(
            "a1".to_string(),
            "login".to_string(),
            vec!["my@email.com".to_string(), "password".to_string()],
        );
        test_login(login_command, login_success).await;
    }

    #[async_std::test]
    async fn test_login_success_command_camel_case() {
        let login_command = Command::new(
            "a1".to_string(),
            "Login".to_string(),
            vec!["my@email.com".to_string(), "password".to_string()],
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
    async fn test_login_bad_creds() {
        let login_command = Command::new(
            "a1".to_string(),
            "LOGIN".to_string(),
            vec!["not.a.user@domain.com".to_string(), "password".to_string()],
        );
        test_login(login_command, login_failed).await;
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
