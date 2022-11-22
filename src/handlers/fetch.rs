// From RFC 9051 (https://www.ietf.org/rfc/rfc9051.html#name-fetch-command):
// C: A654 FETCH 2:4 (FLAGS BODY[HEADER.FIELDS (DATE FROM)])
// S: * 2 FETCH
// + From: someone@example.com
// + To: someone_else@example.com
// + Subject: An RFC 822 formatted message
// +
// + This is a test email body.
// S: * 3 FETCH ....
// S: * 4 FETCH ....
// S: A654 OK FETCH completed

use futures::{SinkExt, StreamExt};

use crate::connection::Request;
use crate::handlers::HandleCommand;
use crate::server::{Command, ParseError, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct FetchHandler {}
#[async_trait::async_trait]
impl HandleCommand for FetchHandler {
    fn name<'a>(&self) -> &'a str {
        "FETCH"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            ()
        }
        if command.num_args() < 1 {
            return Err(Box::new(ParseError {}));
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec![
            Response::from("* 1 FETCH (BODY[TEXT] {26}\r\nThis is a test email body.)").unwrap(),
            Response::new(
                &command.tag(),
                ResponseStatus::OK,
                "FETCH completed.",
            ),
        ])
    }
}
#[async_trait::async_trait]
impl Handle for FetchHandler {
    fn command<'a>(&self) -> &'a str {
        "FETCH"
    }

    async fn start(&mut self, mut requests: Receiver<Request>) -> Result<()> {
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
            request
                .responder
                .send(vec![
                    Response::from("* 1 FETCH (BODY[TEXT] {26}\r\nThis is a test email body.)")
                        .unwrap(),
                    Response::new(
                        &request.command.tag(),
                        ResponseStatus::OK,
                        "FETCH completed.",
                    ),
                ])
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_std::path::PathBuf;

    use super::FetchHandler;
    use crate::auth::User;
    use crate::connection::Context;
    use crate::handlers::tests::test_handle;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, Response, ResponseStatus};

    #[async_std::test]
    async fn test_fetch_success() {
        let fetch_handler = FetchHandler {};
        let fetch_command = Command::new("a1", "FETCH", vec!["1"]);
        let valid = fetch_handler.validate(&fetch_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = fetch_handler.handle(&fetch_command).await;
        fetch_success(response.unwrap());
    }

    #[async_std::test]
    async fn test_fetch_handle() {
        let handler = FetchHandler {};
        let command = Command::new("a1", "FETCH", vec!["1"]);
        test_handle(handler, command, fetch_success, |_|{}, None).await;
    }

    #[async_std::test]
    async fn test_cannot_fetch_if_unselected() {
        let handler = FetchHandler {};
        let command = Command::new("a1", "FETCH", vec!["1"]);
        let ctx = Context::of(Some(User::new("username", "password")), None);
        test_handle(handler, command, |response| {
            assert_eq!(response.len(), 1 as usize);
            assert_eq!(response[0], Response::new("a1", ResponseStatus::NO, "cannot FETCH before SELECT. Please SELECT a folder."))
        }, |_|{}, Some(ctx)).await;
    }

    #[async_std::test]
    async fn test_cannot_fetch_if_unauthenticated() {
        let handler = FetchHandler {};
        let command = Command::new("a1", "FETCH", vec!["1"]);
        let ctx = Context::of(None, Some(PathBuf::from("/this/is/a/folder")));
        test_handle(handler, command, |response| {
            assert_eq!(response.len(), 1 as usize);
            assert_eq!(response[0], Response::new("a1", ResponseStatus::NO, "cannot FETCH when un-authenticated. Please authenticate using LOGIN or AUTHENTICATE."))
        }, |_|{}, Some(ctx)).await;
    }

    fn fetch_success(response: Vec<Response>) {
        assert_eq!(
            response,
            vec!(
                Response::from("* 1 FETCH (BODY[TEXT] {26}\r\nThis is a test email body.)")
                    .unwrap(),
                Response::new(
                    "a1",
                    ResponseStatus::OK,
                    "FETCH completed."
                )
            )
        );
    }
}
