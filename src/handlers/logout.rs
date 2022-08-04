use crate::handlers::{HandleCommand};
use crate::server::{Command, Response, ResponseStatus};
use crate::util::Result;

pub struct LogoutHandler{}
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
        Ok(vec!(Response::new(
            command.tag(),
            ResponseStatus::OK,
            "LOGOUT completed.".to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::LogoutHandler;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, ResponseStatus, Response};
    use crate::util::Result;

    #[async_std::test]
    async fn test_can_login() {
        let logout_handler = LogoutHandler{};
        let logout_command = Command::new(
            "a1".to_string(),
            "LOGOUT".to_string(),
            vec![],
        );
        let valid = logout_handler.validate(&logout_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = logout_handler.handle(&logout_command).await;
        logout_success(response);
    }

    fn logout_success(response: Result<Vec<Response>>) {
        assert_eq!(response.is_ok(), true);
        let r = response.unwrap();
        assert_eq!(r.len(), 1 as usize);
        let reply = &r[0];
        assert_eq!(reply, &Response::new("a1".to_string(), ResponseStatus::OK, "LOGOUT completed.".to_string()));
    }
}