use std::io::{Error, ErrorKind::ConnectionAborted, ErrorKind::WouldBlock, ErrorKind::TimedOut};
use std::fmt::{Formatter, Display, Debug};

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
        return self.cause.kind() == ConnectionAborted;
    }
    pub fn can_ignore(&self) -> bool {
        return self.cause.kind() == WouldBlock || self.cause.kind() == TimedOut;
    }
}

impl Display for IMAPError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        return write!(f, "IMAPError (tag: {}, cause: {})", std::str::from_utf8(&self.tag).unwrap(), self.cause);
    }
}