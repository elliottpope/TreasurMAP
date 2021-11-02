use crate::parser::{Command, CompletionStatus, Parser, Response, IMAPError};
use log::{error, debug};
use std::collections::HashMap;
use std::io::{Read, Write, BufReader, Error, ErrorKind, BufRead};
use std::str::from_utf8;
use std::convert::TryFrom;

// const CAPABILITY: &[u8] = b"capability";

// const NOOP: &[u8] = b"noop" ;

const LOGIN: &[u8] = b"login";
// const LOGOUT: &[u8] = b"logout" ;
// const AUTHENTICATE: &[u8] = b"authenticate" ;

// const STARTTLS: &[u8] = b"starttls" ;

// const SELECT: &[u8] = b"select" ;
// const EXAMINE: &[u8] = b"examine" ;
// const CREATE: &[u8] = b"create" ;
// const DELETE: &[u8] = b"delete" ;
// const RENAME: &[u8] = b"rename" ;

struct LoginHandler {}
pub struct UnknownCommandHandler {}

pub trait Handle: Send + Sync {
    fn handle(&self, command: Command) -> Response;
}

pub trait HandleRequest {
    fn handle<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), IMAPError>;
}

impl Handle for UnknownCommandHandler {
    fn handle(&self, command: Command) -> Response {
        let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
        return Response::new(
            CompletionStatus::BAD,
            tag,
            format!("Command '{}' unknown", command.command),
        );
    }
}

impl Handle for LoginHandler {
    fn handle(&self, command: Command) -> Response {
        assert_eq!(command.command, String::from("login"));
        assert_eq!(command.args.len(), 2);
        if command.args[0] == b"test" && command.args[1] == b"test" {
            let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
            return Response::new(CompletionStatus::OK, tag, String::from("LOGIN completed."));
        } else {
            let tag = String::from(from_utf8(&command.tag).unwrap_or_default());
            return Response::new(CompletionStatus::NO, tag, String::from("LOGIN failed."));
        }
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
        return RequestHandler {
            _buffer_size: buffer_size,
            _parser: parser,
            _delegates: HashMap::new(),
            _default_handler: Box::new(UnknownCommandHandler {}),
        };
    }
}

impl HandleRequest for RequestHandler {
    fn handle<R: Read, W: Write>(&self, mut input: R, output: W) -> Result<(), IMAPError> {
        let buffer_size_usize: usize = usize::try_from(self._buffer_size).expect("Cannot convert buffer size to usize");

        let mut buf: Vec<u8> = Vec::with_capacity(buffer_size_usize);

        match BufReader::new(input.by_ref()).take(self._buffer_size).read_until(b'\n', &mut buf) {
            Ok(read) => {
                debug!("Read {} bytes from source for parsing", read);
                if read == 0 {
                    return Err(IMAPError::new(Vec::new(),
                    Error::new(ErrorKind::ConnectionAborted, "Empty read indicates stream has been closed by client. Server side stream must also be closed.")
                    ))
                }
            },
            Err(e) => {
                let wrapper = IMAPError::new(Vec::new(), e);
                if wrapper.can_ignore() {
                    return Err(wrapper)
                }
                error!("Encountered an error reading from source.");
                return Err(wrapper)
            }
        };
        let command = match self._parser.parse(buf) {
            Ok(cmd) => cmd,
            Err(e) => {
                if e.should_panic() {
                    return Err(e);
                }
                if e.can_ignore() {
                    return Err(e)
                }
                error!("Could not parse input due to {}", e);
                Command {
                    tag: e.tag,
                    command: String::default(),
                    args: Vec::new(),
                }
            }
        };
        match command.command.as_bytes() {
            LOGIN => {

            },
            _ => {
                self._default_handler.handle(command).respond(output);
            } // LOGIN => {
              //     let response = Response::new(CompletionStatus::OK, tag, String::from("LOGIN completed."));
              //     response.respond(output);
              // },
              // LOGOUT => {
              //     let response = Response::new(CompletionStatus::OK, tag, String::from("LOGOUT completed."));
              //     response.respond(output);
              // }
              // _ => {
              //     let response = Response::new(CompletionStatus::BAD, tag, format!("Command '{}' unknown", command.command));
              //     response.respond(output);
              // }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Handle, HandleRequest, LoginHandler, RequestHandler};
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
        response.respond(&mut output);

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
}
