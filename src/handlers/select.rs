// From RFC 9051 (https://www.ietf.org/rfc/rfc9051.html#name-select-command):
//  C: A142 SELECT INBOX
//  S: * 172 EXISTS
//  S: * OK [UIDVALIDITY 3857529045] UIDs valid
//  S: * OK [UIDNEXT 4392] Predicted next UID
//  S: * FLAGS (\Answered \Flagged \Deleted \Seen \Draft)
//  S: * OK [PERMANENTFLAGS (\Deleted \Seen \*)] Limited
//  S: * LIST () "/" INBOX
//  S: A142 OK [READ-WRITE] SELECT completed

use async_std::path::PathBuf;
use futures::{StreamExt, SinkExt};

use crate::connection::{Request, Event};
use crate::handlers::{HandleCommand};
use crate::server::{Command, Response, ResponseStatus, ParseError};
use crate::util::{Result, Receiver};

use super::Handle;

pub struct SelectHandler{}
#[async_trait::async_trait]
impl HandleCommand for SelectHandler {
    fn name<'a>(&self) -> &'a str {
        "SELECT"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            return Ok(())
        }
        if command.num_args() < 1 {
            return Err(Box::new(ParseError{}))
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec!(
            
            Response::from("* 172 EXISTS").unwrap(),
            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
            Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
            Response::from("* LIST () \"/\" INBOX").unwrap(),
            Response::new(&command.tag(), ResponseStatus::OK, "[READ-WRITE] SELECT completed."),
        ))
    }
}
#[async_trait::async_trait]
impl Handle for SelectHandler {
    fn command<'b>(&self) -> &'b str {
        "SELECT"
    }
    async fn start<'b>(&'b mut self, mut requests: Receiver<Request>) -> Result<()> {
        while let Some(mut request) = requests.next().await {
            if let Err(..) = self.validate(&request.command).await {
                request
                    .responder
                    .send(vec![Response::new(
                        &request.command.tag(),
                        ResponseStatus::BAD,
                        "insufficient arguments",
                    )])
                    .await?;
                continue;
            }
            if !request.context.is_authenticated() {
                request.responder.send(vec![Response::new("a1", ResponseStatus::NO, "cannot SELECT when un-authenticated. Please authenticate using LOGIN or AUTHENTICATE.")]).await?;
                continue;
            }
            let folder = request.command.arg(0);
            request.events.send(Event::SELECT(PathBuf::from(folder.clone()))).await?;
            request.responder.send(vec!(
                Response::from("* 172 EXISTS").unwrap(),
                Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
                Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
                Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
                Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
                Response::from(&format!("* LIST () \"/\" {}", folder)).unwrap(),
                Response::new(&request.command.tag(), ResponseStatus::OK, "[READ-WRITE] SELECT completed."),
            )).await?;
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use async_std::path::PathBuf;

    use super::SelectHandler;
    use crate::auth::User;
    use crate::connection::{Context, Event};
    use crate::handlers::tests::test_handle;
    use crate::handlers::{HandleCommand};
    use crate::server::{Command, ResponseStatus, Response};

    #[async_std::test]
    pub async fn test_can_select() {
        let select_handler = SelectHandler{};
        let select_command = Command::new(
            "a1",
            "SELECT",
            vec!["INBOX"],
        );
        let valid = select_handler.validate(&select_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = select_handler.handle(&select_command).await;
        select_success(response.unwrap());
    }

    #[async_std::test]
    async fn test_select_handle() {
        let select_handler = SelectHandler{};
        let command = Command::new(
            "a1",
            "SELECT",
            vec!["INBOX"],
        );

        let ctx = Context::of(Some(User::new("username", "password")), None);
        test_handle(select_handler, command, select_success, Some(|event| {
            match event {
                Event::SELECT(folder) => {
                    assert_eq!(folder, PathBuf::from("INBOX"))
                },
                _ => {
                    panic!("SELECT command should only send SELECT events");
                },
                
            }
        }), Some(ctx)).await;
    }

    #[async_std::test]
    async fn test_cannot_select_if_unauthenticated() {
        let select_handler = SelectHandler{};
        let command = Command::new(
            "a1",
            "SELECT",
            vec!["INBOX"],
        );

        let mut f = Some(|_event|{});
        f.take();
        test_handle(select_handler, command, |response| {
            assert_eq!(response.len(), 1);
            assert_eq!(response[0], Response::new("a1", ResponseStatus::NO, "cannot SELECT when un-authenticated. Please authenticate using LOGIN or AUTHENTICATE."));
        }, f, None).await;
    }

    #[async_std::test]
    async fn test_select_bad_args() {
        let select_handler = SelectHandler{};

        let command = Command::new(
            "a1",
            "SELECT",
            vec![],
        );
        let mut f = Some(|_event|{});
        f.take();
        test_handle(select_handler, command, |response| {
            assert_eq!(response.len(), 1);
            assert_eq!(response[0], Response::new("a1", ResponseStatus::BAD, "insufficient arguments"));
        }, f, None).await;
    }

    fn select_success(response: Vec<Response>) {
        assert_eq!(response, vec!(
            Response::from("* 172 EXISTS").unwrap(),
            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
            Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
            Response::from("* LIST () \"/\" INBOX").unwrap(),
            Response::new("a1", ResponseStatus::OK, "[READ-WRITE] SELECT completed.")));
    }
}
