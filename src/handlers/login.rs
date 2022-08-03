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
            "completed".to_string(),
        ))
    }
}