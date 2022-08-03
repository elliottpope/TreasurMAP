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
        Ok(vec!(Response::new(command.tag(), ResponseStatus::OK, command.command(), "completed.".to_string())))
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
        let reply = &response.unwrap()[0];
        assert_eq!(reply, &Response::new("a1".to_string(), ResponseStatus::OK, "SELECT".to_string(), "completed.".to_string()));
    }
}
