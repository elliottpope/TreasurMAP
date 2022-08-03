use crate::handlers::{HandleCommand};
use crate::server::{Command, Response, ResponseStatus};
use crate::util::Result;

pub struct LoginHandler{}
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
            // return error
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Response> {
        // TODO: implement user database lookup
        // TODO: add user to some state management
        let mut _user = command.arg(0);
        let _password = &command.arg(1);
        _user = _user.replace("\"", "");
        Ok(Response::new(
            command.tag(),
            ResponseStatus::OK,
            command.command(),
            "completed.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::LoginHandler;
    use crate::handlers::HandleCommand;
    use crate::server::{Command, ResponseStatus};

    #[async_std::test]
    async fn test_can_login() {
        let login_handler = LoginHandler{};
        let login_command = Command::new(
            "a1".to_string(),
            "LOGIN".to_string(),
            vec!["my@email.com".to_string(), "password".to_string()],
        );
        let valid = login_handler.validate(&login_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = login_handler.handle(&login_command).await;
        assert_eq!(response.is_ok(), true);
        let reply = response.unwrap();
        assert_eq!(reply.tag(), "a1".to_string());
        assert_eq!(reply.status(), ResponseStatus::OK);
        assert_eq!(reply.command(), "LOGIN".to_string());
        assert_eq!(reply.message(), "completed.".to_string());
    }
}