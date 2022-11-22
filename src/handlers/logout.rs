use futures::{SinkExt, StreamExt};

use crate::connection::Request;
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
            command.tag(),
            ResponseStatus::OK,
            "LOGOUT completed.",
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
                        request.command.tag(),
                        ResponseStatus::BAD,
                        "cannot understand LOGOUT command provided",
                    )])
                    .await?;
                continue;
            }
            request
                .responder
                .send(vec![Response::new(
                    request.command.tag(),
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
        test_handle(handler, command, logout_success).await;
    }

    fn logout_success(response: Vec<Response>) {
        assert_eq!(response.len(), 1 as usize);
        let reply = &response[0];
        assert_eq!(
            reply,
            &Response::new(
                "a1".to_string(),
                ResponseStatus::OK,
                "LOGOUT completed."
            )
        );
    }
}
