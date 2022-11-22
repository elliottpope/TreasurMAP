use futures::{SinkExt, StreamExt};

use crate::connection::{Request, Event};
use crate::handlers::HandleCommand;
use crate::server::{Command, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct LogoutHandler {}
#[async_trait::async_trait]
impl HandleCommand for LogoutHandler {
    fn name<'a>(&self) -> &'a str {
        "LOGOUT"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            ()
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec![Response::new(
            &command.tag(),
            ResponseStatus::OK,
            "LOGOUT completed. Goodbye!",
        )])
    }
}
#[async_trait::async_trait]
impl Handle for LogoutHandler {
    fn command<'b>(&self) -> &'b str {
        "LOGOUT"
    }
    async fn start<'b>(&'b mut self, mut requests: Receiver<Request>) -> Result<()> {
        while let Some(mut request) = requests.next().await {
            if let Err(..) = self.validate(&request.command).await {
                request
                    .responder
                    .send(vec![Response::new(
                        &request.command.tag(),
                        ResponseStatus::BAD,
                        "cannot understand LOGOUT command provided",
                    )])
                    .await?;
                continue;
            }
            request.events.send(Event::UNAUTH()).await?;
            request
                .responder
                .send(vec![Response::new(
                    &request.command.tag(),
                    ResponseStatus::OK,
                    "LOGOUT completed. Goodbye!",
                )])
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LogoutHandler;
    use crate::connection::Event;
    use crate::handlers::tests::test_handle;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, Response, ResponseStatus};

    #[async_std::test]
    async fn test_can_login() {
        let logout_handler = LogoutHandler {};
        let logout_command = Command::new("a1", "LOGOUT", vec![]);
        let valid = logout_handler.validate(&logout_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = logout_handler.handle(&logout_command).await;
        logout_success(response.unwrap());
    }

    #[async_std::test]
    async fn test_can_logout() {
        let handler = LogoutHandler {};
        let command = Command::new("a1", "LOGOUT", vec![]);

        let f = Some(|event|{
            match event {
                Event::UNAUTH() => {},
                _ => panic!("LogoutHandler should only send UNAUTH events"),
            }
        });
        test_handle(handler, command, logout_success, f, None).await;
    }

    fn logout_success(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new(
                "a1",
                ResponseStatus::OK,
                "LOGOUT completed. Goodbye!"
            )
        );
    }
}
