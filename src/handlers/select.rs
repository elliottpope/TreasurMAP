// From RFC 9051 (https://www.ietf.org/rfc/rfc9051.html#name-select-command):
//  C: A142 SELECT INBOX
//  S: * 172 EXISTS
//  S: * OK [UIDVALIDITY 3857529045] UIDs valid
//  S: * OK [UIDNEXT 4392] Predicted next UID
//  S: * FLAGS (\Answered \Flagged \Deleted \Seen \Draft)
//  S: * OK [PERMANENTFLAGS (\Deleted \Seen \*)] Limited
//  S: * LIST () "/" INBOX
//  S: A142 OK [READ-WRITE] SELECT completed

use crate::handlers::{HandleCommand};
use crate::server::{Command, Response, ResponseStatus, ParseError};
use crate::util::Result;

pub struct SelectHandler{}
#[async_trait::async_trait]
impl HandleCommand for SelectHandler {
    fn name<'a>(&self) -> &'a str {
        "SELECT"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            return Ok(())
        }
        if command.num_args() < 1 {
            return Err(Box::new(ParseError{}))
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec!(
            
            Response::from("* 172 EXISTS").unwrap(),
            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
            Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
            Response::from("* LIST () \"/\" INBOX").unwrap(),
            Response::new(command.tag(), ResponseStatus::OK, "[READ-WRITE] SELECT completed.".to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::SelectHandler;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, ResponseStatus, Response};
    use crate::util::Result;

    #[async_std::test]
    async fn test_can_login() {
        let select_handler = SelectHandler{};
        let select_command = Command::new(
            "a1".to_string(),
            "SELECT".to_string(),
            vec!["INBOX".to_string()],
        );
        let valid = select_handler.validate(&select_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = select_handler.handle(&select_command).await;
        select_success(response);
    }

    fn select_success(response: Result<Vec<Response>>) {
        assert_eq!(response.is_ok(), true);
        assert_eq!(response.unwrap(), vec!(
            Response::from("* 172 EXISTS").unwrap(),
            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
            Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
            Response::from("* LIST () \"/\" INBOX").unwrap(),
            Response::new("a1".to_string(), ResponseStatus::OK, "[READ-WRITE] SELECT completed.".to_string())));
    }
}
