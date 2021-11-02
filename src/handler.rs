use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufRead, BufReader, Error, ErrorKind, Read, Write};
use std::str::from_utf8;

use log::{debug, error};

use crate::auth::{Authenticator, BasicAuthenticator};
use crate::error::IMAPError;
use crate::parser::{Command, CompletionStatus, Parser, Response};

// const NOOP: &[u8] = b"noop" ;

// const LOGOUT: &[u8] = b"logout" ;
// const AUTHENTICATE: &[u8] = b"authenticate" ;

// const STARTTLS: &[u8] = b"starttls" ;

// const SELECT: &[u8] = b"select" ;
// const EXAMINE: &[u8] = b"examine" ;
// const CREATE: &[u8] = b"create" ;
// const DELETE: &[u8] = b"delete" ;
// const RENAME: &[u8] = b"rename" ;

struct LoginHandler {}
struct CapabilityHandler {}
struct NoopHandler {}
struct LogoutHandler {}
struct StartTLSHandler {}
struct AuthenticateHandler {}
struct SelectHandler {}
struct ExamineHandler {}
struct CreateHandler {}
struct DeleteHandler {}
struct RenameHandler {}
struct SubscribeHandler {}
struct UnsubscribeHandler {}
struct ListHandler {}
struct LsubHandler {}
struct StatusHandler {}
struct AppendHandler {}
struct CheckHandler {}
struct CloseHandler {}
struct ExpungeHandler {}
struct SearchHandler {}
struct FetchHandler {}
struct StoreHandler {}
struct CopyHandler {}
struct UIDHandler {}
pub struct UnknownCommandHandler {}

impl Handle for NoopHandler {}
impl Handle for LogoutHandler {}
impl Handle for StartTLSHandler {}
impl Handle for AuthenticateHandler {}
impl Handle for SelectHandler {}
impl Handle for ExamineHandler {}
impl Handle for CreateHandler {}
impl Handle for DeleteHandler {}
impl Handle for RenameHandler {}
impl Handle for SubscribeHandler {}
impl Handle for UnsubscribeHandler {}
impl Handle for ListHandler {}
impl Handle for LsubHandler {}
impl Handle for StatusHandler {}
impl Handle for AppendHandler {}
impl Handle for CheckHandler {}
impl Handle for CloseHandler {}
impl Handle for ExpungeHandler {}
impl Handle for SearchHandler {}
impl Handle for FetchHandler {}
impl Handle for StoreHandler {}
impl Handle for CopyHandler {}
impl Handle for UIDHandler {}

pub trait Handle: Send + Sync {
    fn handle(&self, command: Command) -> Vec<Response> {
        Vec::from([Response::new(
            CompletionStatus::NO,
            String::from(from_utf8(&command.tag).unwrap_or_default()),
            String::from("Not implemented yet"),
        )])
    }
}

pub trait HandleRequest {
    fn handle<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), IMAPError>;
}

impl Handle for UnknownCommandHandler {
    fn handle(&self, command: Command) -> Vec<Response> {
        let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
        return Vec::from([Response::new(
            CompletionStatus::BAD,
            tag,
            format!("Command '{}' unknown", command.command),
        )]);
    }
}

impl Handle for LoginHandler {
    fn handle(&self, command: Command) -> Vec<Response> {
        assert_eq!(command.command, String::from("login"));
        if command.args.len() < 2 {
            return Vec::from([Response::new(
                CompletionStatus::BAD,
                String::from(from_utf8(&command.tag).unwrap_or_default()),
                String::from(
                    "Missing argument. Command should be '<tag> LOGIN <userid> <password>'",
                ),
            )]);
        }
        let authenticator = BasicAuthenticator::new(
            String::from(from_utf8(&command.args[0]).unwrap_or_default()),
            String::from(from_utf8(&command.args[1]).unwrap_or_default()),
        );
        let response = match authenticator.authenticate() {
            Some(_) => {
                // TODO: store returned User object into session context
                let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
                Response::new(CompletionStatus::OK, tag, String::from("LOGIN completed."))
            }
            None => {
                let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
                Response::new(CompletionStatus::NO, tag, String::from("LOGIN failed."))
            }
        };
        Vec::from([response])
    }
}

impl Handle for CapabilityHandler {
    fn handle(&self, command: Command) -> Vec<Response> {
        assert_eq!(command.command, String::from("capability"));
        Vec::from([
            Response::new(
                CompletionStatus::OK,
                String::default(),
                String::from("CAPABILITY IMAP4rev1"),
            ),
            Response::new(
                CompletionStatus::OK,
                String::from(from_utf8(&command.tag).unwrap_or_default()),
                String::from("CAPABILITY completed"),
            ),
        ])
    }
}

pub struct RequestHandler {
    _buffer_size: u64,
    _parser: Parser,
    _delegates: HashMap<String, Box<dyn Handle>>,
    _default_handler: Box<dyn Handle>,
}

impl RequestHandler {
    pub fn new(buffer_size: u64, parser: Parser) -> RequestHandler {
        let mut handlers: HashMap<String, Box<dyn Handle>> = HashMap::new();
        handlers.insert(String::from("capability"), Box::new(CapabilityHandler {}));
        handlers.insert(String::from("login"), Box::new(LoginHandler {}));
        return RequestHandler {
            _buffer_size: buffer_size,
            _parser: parser,
            _delegates: handlers,
            _default_handler: Box::new(UnknownCommandHandler {}),
        };
    }
}

impl HandleRequest for RequestHandler {
    fn handle<R: Read, W: Write>(&self, mut input: R, mut output: W) -> Result<(), IMAPError> {
        let buffer_size_usize: usize =
            usize::try_from(self._buffer_size).expect("Cannot convert buffer size to usize");

        let mut buf: Vec<u8> = Vec::with_capacity(buffer_size_usize);

        match BufReader::new(input.by_ref())
            .take(self._buffer_size)
            .read_until(b'\n', &mut buf)
        {
            Ok(read) => {
                debug!("Read {} bytes from source for parsing", read);
                if read == 0 {
                    return Err(IMAPError::new(Vec::new(),
                    Error::new(ErrorKind::ConnectionAborted, "Empty read indicates stream has been closed by client. Server side stream must also be closed.")
                    ));
                }
            }
            Err(e) => {
                let wrapper = IMAPError::new(Vec::new(), e);
                if wrapper.can_ignore() {
                    return Err(wrapper);
                }
                error!("Encountered an error reading from source.");
                return Err(wrapper);
            }
        };
        let command = match self._parser.parse(buf) {
            Ok(cmd) => cmd,
            Err(e) => {
                if e.should_panic() {
                    return Err(e);
                }
                if e.can_ignore() {
                    return Err(e);
                }
                error!("Could not parse input due to {}", e);
                Command {
                    tag: e.tag,
                    command: String::default(),
                    args: Vec::new(),
                }
            }
        };
        match self._delegates.get(&command.command) {
            Some(handler) => handler.handle(command),
            None => self._default_handler.handle(command),
        }
        .iter()
        .for_each(|response| response.respond(&mut output));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityHandler, Handle, HandleRequest, LoginHandler, RequestHandler};
    use crate::parser::{Command, Parser};
    use std::str::from_utf8;
    use std::vec::Vec;

    #[test]
    fn test_handler_can_login_success() {
        let handler = LoginHandler {};
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
            from_utf8(output.as_ref()).unwrap(),
            "tag1 OK LOGIN completed.\n"
        );
    }
    #[test]
    fn test_request_handler_handles_unknown_tag() {
        let handler = RequestHandler::new(128, Parser::new());
        let mut input = b"tag1 command arg1 arg2" as &[u8];
        let mut output: Vec<u8> = Vec::with_capacity(128);

        handler
            .handle(&mut input, &mut output)
            .expect("Input stream was closed unexpectedly");

        assert_eq!(
            from_utf8(output.as_ref()).unwrap(),
            "tag1 BAD Command 'command' unknown\n"
        );
    }

    #[test]
    fn test_handle_buffer_overflow() {
        let handler = RequestHandler::new(16, Parser::new());

        let mut input = b"thisstringofbytesistoolong" as &[u8];
        let mut output: Vec<u8> = Vec::with_capacity(128);

        handler
            .handle(&mut input, &mut output)
            .expect("Input stream was closed unexpectedly");

        assert_eq!(
            from_utf8(output.as_ref()).unwrap(),
            "thisstringofbyte BAD Command '' unknown\n"
        );
    }

    #[test]
    fn test_ok_capability_response() {
        let handler = CapabilityHandler {};

        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("capability"),
            args: Vec::new(),
        };

        let responses = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        responses
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            from_utf8(output.as_ref()).unwrap(),
            "* OK CAPABILITY IMAP4rev1\ntag1 OK CAPABILITY completed\n"
        );
    }

    #[test]
    fn test_can_login_successfully() {
        let handler = LoginHandler {};

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
            from_utf8(output.as_ref()).unwrap(),
            "tag1 OK LOGIN completed.\n"
        );
    }

    #[test]
    fn test_login_no_password_provided_fails() {
        let handler = LoginHandler {};

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
            from_utf8(output.as_ref()).unwrap(),
            "tag1 BAD Missing argument. Command should be '<tag> LOGIN <userid> <password>'\n"
        );
    }

    #[test]
    fn test_login_authentication_fails() {
        let handler = LoginHandler {};

        let command = Command {
            tag: Vec::from(b"tag1" as &[u8]),
            command: String::from("login"),
            args: Vec::from([Vec::from(b"test" as &[u8]), Vec::from(b"badpassword" as &[u8])]),
        };

        let responses = handler.handle(command);

        let mut output: Vec<u8> = Vec::with_capacity(64);
        responses
            .iter()
            .for_each(|response| response.respond(&mut output));

        assert_eq!(
            from_utf8(output.as_ref()).unwrap(),
            "tag1 NO LOGIN failed.\n"
        );
    }

}
