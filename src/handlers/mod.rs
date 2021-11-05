pub mod login_handler;

use std::str::from_utf8;

use crate::parser::{Command, CompletionStatus, Response};

// const NOOP: &[u8] = b"noop" ;

// const LOGOUT: &[u8] = b"logout" ;
// const AUTHENTICATE: &[u8] = b"authenticate" ;

// const STARTTLS: &[u8] = b"starttls" ;

// const SELECT: &[u8] = b"select" ;
// const EXAMINE: &[u8] = b"examine" ;
// const CREATE: &[u8] = b"create" ;
// const DELETE: &[u8] = b"delete" ;
// const RENAME: &[u8] = b"rename" ;

pub struct CapabilityHandler {}
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

impl Handle for CapabilityHandler {
    fn handle(&self, command: Command) -> Vec<Response> {
        assert_eq!(command.command, String::from("capability"));
        Vec::from([
            Response::new(
                CompletionStatus::OK,
                String::default(),
                String::from("CAPABILITY IMAP4rev1"),
            ),
            Response::from(
                CompletionStatus::OK,
                &command.tag,
                "CAPABILITY completed",
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityHandler, Handle};
    use crate::parser::{Command};
    use std::str::from_utf8;
    use std::vec::Vec;


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

}
