


use crate::auth::{BasicAuth, BasicAuthenticator, Authenticator};
use crate::handlers::{Handle};
use crate::parser::{Command, Response, CompletionStatus};

pub struct LoginHandler {
    pub _authenticator: BasicAuthenticator,
}


impl Handle for LoginHandler {
    fn handle(&self, command: Command) -> Vec<Response> {
        assert_eq!(command.command, String::from("login"));
        if command.args.len() < 2 {
            return Vec::from([Response::new(
                CompletionStatus::BAD,
                String::from_utf8_lossy(&command.tag).to_string(),
                String::from(
                    "Missing argument. Command should be '<tag> LOGIN <userid> <password>'",
                ),
            )]);
        }
        let response = match self._authenticator.authenticate(BasicAuth::from(&command.args[0], &command.args[1])) {
            Some(_) => {
                // TODO: store returned User object into session context
                let tag = String::from_utf8_lossy(&command.tag).to_string();
                Response::new(CompletionStatus::OK, tag, String::from("LOGIN completed."))
            }
            None => {
                let tag = String::from_utf8_lossy(&command.tag).to_string();
                Response::new(CompletionStatus::NO, tag, String::from("LOGIN failed."))
            }
        };
        Vec::from([response])
    }
}

#[cfg(test)]
mod tests {
    use super::LoginHandler;

    use crate::auth::{BasicAuthenticator, InMemoryUserStore};
    use crate::parser::{Command};
    use crate::handlers::{Handle};

    #[test]
    fn test_handler_can_login_success() {
        let handler = LoginHandler {
            _authenticator: BasicAuthenticator::new(InMemoryUserStore::new().with_user(String::from("test"), String::from("test")))
        };
        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("login"),
            args: Vec::from([Vec::from(b"test" as &[u8]), Vec::from(b"test" as &[u8])]),
        };

        let response = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        response
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            String::from_utf8_lossy(&output).to_string(),
            "tag1 OK LOGIN completed.\n"
        );
    }
    #[test]
    fn test_can_login_successfully() {
        let handler = LoginHandler {
            _authenticator: BasicAuthenticator::new(InMemoryUserStore::new().with_user(String::from("test"), String::from("test")))
        };

        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("login"),
            args: Vec::from([Vec::from(b"test" as &[u8]), Vec::from(b"test" as &[u8])]),
        };

        let responses = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        responses
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            String::from_utf8_lossy(&output).to_string(),
            "tag1 OK LOGIN completed.\n"
        );
    }

    #[test]
    fn test_login_no_password_provided_fails() {
        let handler = LoginHandler {
            _authenticator: BasicAuthenticator::new(InMemoryUserStore::new().with_user(String::from("test"), String::from("test")))
        };

        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("login"),
            args: Vec::from([Vec::from(b"test" as &[u8])]),
        };

        let responses = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        responses
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            String::from_utf8_lossy(&output).to_string(),
            "tag1 BAD Missing argument. Command should be '<tag> LOGIN <userid> <password>'\n"
        );
    }

    #[test]
    fn test_login_authentication_fails() {
        let handler = LoginHandler {
            _authenticator: BasicAuthenticator::new(InMemoryUserStore::new().with_user(String::from("test"), String::from("test")))
        };

        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("login"),
            args: Vec::from([
                Vec::from(b"test" as &[u8]),
                Vec::from(b"badpassword" as &[u8]),
            ]),
        };

        let responses = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        responses
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            String::from_utf8_lossy(&output).to_string(),
            "tag1 NO LOGIN failed.\n"
        );
    }
}