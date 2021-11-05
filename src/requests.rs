use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufRead, BufReader, Error, ErrorKind::ConnectionAborted, Read, Write};

use log::{debug, error};

use crate::auth::{BasicAuthenticator, InMemoryUserStore, UserStore};
use crate::error::IMAPError;
use crate::handlers::{CapabilityHandler, Handle, login_handler::LoginHandler, UnknownCommandHandler};
use crate::parser::{Command, Parser};

const DEFAULT_BUFFER_SIZE: u64 = 1024;

pub trait HandleRequest {
    fn handle<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), IMAPError>;
}

pub struct RequestHandler {
    _buffer_size: u64,
    _parser: Parser,
    _delegates: HashMap<String, Box<dyn Handle>>,
    _default_handler: Box<dyn Handle>,
}

pub struct RequestHandlerBuilder {
    _buffer_size: u64,
    _parser: Option<Parser>,
    _handlers: HashMap<String, Box<dyn Handle>>,
    _default: Option<Box<dyn Handle>>,
    _user_store: Option<Box<dyn UserStore>>,
}

impl RequestHandlerBuilder {
    pub fn new() -> RequestHandlerBuilder {
        RequestHandlerBuilder {
            _handlers: HashMap::new(),
            _default: None,
            _user_store: None,
            _buffer_size: 0,
            _parser: None,
        }
    }
    pub fn with_handler<H: Handle + 'static>(
        mut self,
        command: String,
        handler: H,
    ) -> RequestHandlerBuilder {
        self._handlers.insert(command, Box::new(handler));
        self
    }
    pub fn with_default_handler<H: Handle + 'static>(
        mut self,
        handler: H,
    ) -> RequestHandlerBuilder {
        self._default.replace(Box::new(handler));
        self
    }
    pub fn with_user_store<S: UserStore + 'static>(mut self, store: S) -> RequestHandlerBuilder {
        self._handlers.insert(
            String::from("login"),
            Box::new(LoginHandler {
                _authenticator: BasicAuthenticator::new(store),
            }),
        );
        self
    }
    pub fn with_buffer_size(mut self, size: u64) -> RequestHandlerBuilder {
        self._buffer_size = size;
        self
    }
    pub fn with_parser(mut self, parser: Parser) -> RequestHandlerBuilder {
        self._parser.replace(parser);
        self
    }
    pub fn build(mut self) -> RequestHandler {
        let store =
            InMemoryUserStore::new().with_user(String::from("admin"), String::from("admin"));
        if !self._handlers.contains_key(&String::from("login")) {
            self = self.with_handler(
                String::from("login"),
                LoginHandler {
                    _authenticator: BasicAuthenticator::new(store),
                },
            );
        }
        self = self.with_handler("capability".to_string(), CapabilityHandler {});
        // there are more elegant ways to do this but this gives us a clean compile
        if self._default.is_none() {
            self = self.with_default_handler(UnknownCommandHandler {});
        }
        RequestHandler {
            _delegates: self._handlers,
            // can unwrap here because we inserted the UnknownCommandHandler above
            _default_handler: self._default.unwrap(),
            _buffer_size: match self._buffer_size > 0 {
                true => self._buffer_size,
                false => DEFAULT_BUFFER_SIZE,
            },
            _parser: match self._parser {
                Some(parser) => parser,
                None => Parser::new(),
            },
        }
    }
}

impl HandleRequest for RequestHandler {
    fn handle<R: Read, W: Write>(&self, mut input: R, mut output: W) -> Result<(), IMAPError> {
        let buffer_size_usize: usize =
            usize::try_from(self._buffer_size).expect("Cannot convert buffer size to usize");

        let mut buf: String = String::with_capacity(buffer_size_usize);

        match BufReader::new(input.by_ref())
            .take(self._buffer_size)
            .read_line(&mut buf)
        {
            Ok(read) => {
                debug!("Read {} bytes from source for parsing", read);
                if read == 0 {
                    return Err(IMAPError::new(Vec::new(),
                    Error::new(ConnectionAborted, "Empty read indicates stream has been closed by client. Server side stream must also be closed.")
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
        let command = match self._parser.parse(Vec::from(buf.as_bytes())) {
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

use std::collections::HashMap;
use std::boxed::Box;
use std::iter::FromIterator;

use super::{RequestHandler, HandleRequest};

use crate::parser::Parser;
use crate::handlers::{CapabilityHandler, UnknownCommandHandler, Handle};

    #[test]
    fn test_request_handler_handles_unknown_tag() {
        let handler = RequestHandler {
            _buffer_size: 32,
            _parser: Parser::new(),
            _delegates: HashMap::new(),
            _default_handler: Box::new(UnknownCommandHandler {}),
        };
        let mut input = b"tag1 command arg1 arg2" as &[u8];
        let mut output: Vec<u8> = Vec::with_capacity(128);

        handler
            .handle(&mut input, &mut output)
            .expect("Input stream was closed unexpectedly");

        assert_eq!(
            String::from_utf8_lossy(&output),
            "tag1 BAD Command 'command' unknown\n"
        );
    }

    #[test]
    fn test_request_handler_no_args() {
        let capability_handler = Box::new(CapabilityHandler {}) as Box<dyn Handle>;
        let handlers: HashMap<String, Box<dyn Handle>> =
            HashMap::from_iter([("capability".to_string(), capability_handler)]);
        let handler = RequestHandler {
            _buffer_size: 32,
            _parser: Parser::new(),
            _delegates: handlers,
            _default_handler: Box::new(UnknownCommandHandler {}),
        };

        let mut input = b"tag1 capability\n" as &[u8];
        let mut output: Vec<u8> = Vec::with_capacity(32);

        handler
            .handle(&mut input, &mut output)
            .expect("Input stream was closed unexpectedly");

        assert_eq!(
            String::from_utf8_lossy(&output),
            "* OK CAPABILITY IMAP4rev1\ntag1 OK CAPABILITY completed\n"
        );
    }

    #[test]
    fn test_handle_buffer_overflow() {
        let handler = RequestHandler {
            _buffer_size: 16,
            _parser: Parser::new(),
            _delegates: HashMap::new(),
            _default_handler: Box::new(UnknownCommandHandler {}),
        };
        let mut input = b"thisstringofbytesistoolong" as &[u8];
        let mut output: Vec<u8> = Vec::with_capacity(128);

        handler
            .handle(&mut input, &mut output)
            .expect("Input stream was closed unexpectedly");

        assert_eq!(
            String::from_utf8_lossy(&output),
            "thisstringofbyte BAD Command '' unknown\n"
        );
    }
}
