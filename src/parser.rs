use std::io::{ Write, ErrorKind, Error};
use std::fmt::{Display, Formatter};
use std::str::from_utf8;

#[derive(Debug)]
pub struct Command {
    pub tag: Vec<u8>,
    pub command: String,
    pub args: Vec<Vec<u8>>
}

pub struct Parser {
}

pub struct Response {
    status: CompletionStatus,
    tag: String,
    response: String
}

impl Response {
    pub fn new(status: CompletionStatus, tag: String, response: String) -> Response {
        return Response {
            status: status,
            tag: tag,
            response: response
        }
    }
}

#[derive(Debug)]
pub struct IMAPError {
    pub tag: Vec<u8>,
    cause: Error
}

impl IMAPError {
    pub fn new(tag: Vec<u8>, cause: Error) -> IMAPError {
        return IMAPError {
            tag: tag,
            cause: cause
        }
    }
    pub fn should_panic(&self) -> bool {
        return self.cause.kind() == ErrorKind::ConnectionAborted;
    }
    pub fn can_ignore(&self) -> bool {
        return self.cause.kind() == ErrorKind::WouldBlock || self.cause.kind() == ErrorKind::TimedOut;
    }
}

impl Display for IMAPError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        return write!(f, "IMAPError (tag: {}, cause: {})", std::str::from_utf8(&self.tag).unwrap(), self.cause);
    }
}

pub enum CompletionStatus {
    OK, NO, BAD
}

impl Display for CompletionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        return match self {
            CompletionStatus::OK => write!(f, "{}", "OK"),
            CompletionStatus::BAD => write!(f, "{}", "BAD"),
            CompletionStatus::NO => write!(f, "{}", "NO"),
        }
    }
}

impl Response {
    pub fn respond<W: Write>(&self, mut output: W) {
        let tag = if self.tag.as_str() == "" { "*" } else { self.tag.as_str() };
        let response_string = format!("{} {} {}\n", tag, self.status, self.response);
        output.write(response_string.as_ref()).expect("Unable to write response");
        // output.flush().expect("Unable to write response");
    }
}

impl Parser {
    pub fn new() -> Parser {
        return Parser {
        }
    }

    pub fn parse(&self, input: Vec<u8>) -> std::result::Result<Command, IMAPError> {
        let mut split_input = input.split(|b| {b == &b' '});
        
        let input_tag = Vec::from(match split_input.next() {
            Some(bytes) => bytes,
            None => {
                return Err(IMAPError {
                    tag: Vec::new(),
                    cause: Error::new(ErrorKind::InvalidInput, "Tag not provided. Expected <tag SPACE command [arguments]>")
                })
            }
        });
        let input_command = match split_input.next() {
            Some(bytes) => {
                String::from(match from_utf8(bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        return Err(IMAPError {
                            tag: Vec::from(input_tag),
                            cause: Error::new(ErrorKind::InvalidInput, "Provided command is not a valid UTF-8 string")
                        })
                    }
                }).to_lowercase()
            },
            None => {
                return Err(IMAPError {
                    tag: Vec::from(input_tag),
                    cause: Error::new(ErrorKind::InvalidInput, "Command not provided. Expected <tag SPACE command [arguments]>")
                })
            }
        };
        let mut input_args = Vec::new();
        while let Some(arg) = split_input.next() {
            input_args.push(Vec::from(arg));
        }
        return Ok(Command {
            tag: input_tag,
            command: input_command,
            args: input_args
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Parser, Response, CompletionStatus};
    use std::vec::Vec;
    use std::str;

    #[test]
    fn test_can_read_tag() {
        let parser = Parser::new();

        let input = Vec::from(b"tag1 other data" as &[u8]);
        let command = parser.parse(input);

        assert!(command.is_ok());
        let command = command.unwrap();
        assert_eq!(command.tag, b"tag1");
        assert_eq!(command.command, "other");
        assert_eq!(command.args[0], b"data");
    }
    #[test]
    fn test_command_uppercase_is_lowered() {
        let parser = Parser::new();

        let input = Vec::from(b"tag1 OTHER data" as &[u8]);
        let command = parser.parse(input);

        assert!(command.is_ok());
        let command = command.unwrap();
        assert_eq!(command.tag, b"tag1");
        assert_eq!(command.command, "other");
        assert_eq!(command.args[0], b"data");
    }

    #[test]
    fn test_handle_non_utf8() {        
        let parser = Parser::new();
        let data = [b'\xC0', b'\xC1', b'\xF5', b'\xF6', b'\xF7', b'\xF8', 
            b'\xF9', b'\xFA', b'\xFB', b'\xFC', b'\xFD', b'\xFE', b'\xFF', b' '];
        let input = Vec::from(&data as &[u8]);

        let command = parser.parse(input);

        assert!(command.is_ok());
        assert_eq!(command.unwrap().tag, data[0..13]);
    }

    #[test]
    fn test_ok_response_can_be_output() {
        let response = Response {
            status: CompletionStatus::OK,
            tag: String::from("tag1"),
            response: String::from("Your response")
        };

        let mut output: Vec<u8> = Vec::with_capacity(32);
        response.respond(&mut output);

        assert_eq!(str::from_utf8(output.as_ref()).unwrap(), "tag1 OK Your response\n");
    }
    #[test]
    fn test_no_response_can_be_output() {
        let response = Response {
            status: CompletionStatus::NO,
            tag: String::from("tag1"),
            response: String::from("Your response")
        };

        let mut output: Vec<u8> = Vec::with_capacity(32);
        response.respond(&mut output);

        assert_eq!(str::from_utf8(output.as_ref()).unwrap(), "tag1 NO Your response\n");
    }
    #[test]
    fn test_bad_response_can_be_output() {
        let response = Response {
            status: CompletionStatus::BAD,
            tag: String::from("tag1"),
            response: String::from("Your response")
        };

        let mut output: Vec<u8> = Vec::with_capacity(32);
        response.respond(&mut output);

        assert_eq!(str::from_utf8(output.as_ref()).unwrap(), "tag1 BAD Your response\n");
    }
    #[test]
    fn test_untagged_response_can_be_output() {
        let response = Response {
            status: CompletionStatus::OK,
            tag: String::from(""),
            response: String::from("Your response")
        };

        let mut output: Vec<u8> = Vec::with_capacity(32);
        response.respond(&mut output);

        assert_eq!(str::from_utf8(output.as_ref()).unwrap(), "* OK Your response\n");
    }
}
