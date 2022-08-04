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

use crate::handlers::HandleCommand;
use crate::server::{Command, Response, ParseError, ResponseStatus};
use crate::util::Result;

pub struct FetchHandler{}
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
            return Err(Box::new(ParseError{}))
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec!(
            Response::from("* 1 FETCH (BODY[TEXT] {26}\r\nThis is a test email body.)").unwrap(),
            Response::new(command.tag(), ResponseStatus::OK, "FETCH completed.".to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::FetchHandler;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, Response, ResponseStatus};
    use crate::util::Result;

    #[async_std::test]
    async fn test_fetch_success() {
        let fetch_handler = FetchHandler{};
        let fetch_command = Command::new(
            "a1".to_string(),
            "FETCH".to_string(),
            vec!["1".to_string()],
        );
        let valid = fetch_handler.validate(&fetch_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = fetch_handler.handle(&fetch_command).await;
        fetch_success(response);
    }

    fn fetch_success(response: Result<Vec<Response>>) {
        assert_eq!(response.is_ok(), true);
        let r = response.unwrap();
        assert_eq!(r, vec!(
            Response::from("* 1 FETCH (BODY[TEXT] {26}\r\nThis is a test email body.)").unwrap(),
            Response::new("a1".to_string(), ResponseStatus::OK, "FETCH completed.".to_string())
        ));
    }
}