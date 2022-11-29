// From RFC 9051 (https://www.ietf.org/rfc/rfc9051.html#name-select-command):
//  C: A142 SELECT INBOX
//  S: * 172 EXISTS
//  S: * OK [UIDVALIDITY 3857529045] UIDs valid
//  S: * OK [UIDNEXT 4392] Predicted next UID
//  S: * FLAGS (\Answered \Flagged \Deleted \Seen \Draft)
//  S: * OK [PERMANENTFLAGS (\Deleted \Seen \*)] Limited
//  S: * LIST () "/" INBOX
//  S: A142 OK [READ-WRITE] SELECT completed

use std::borrow::BorrowMut;

use async_std::path::PathBuf;
use futures::channel::mpsc::UnboundedSender;
use futures::channel::oneshot::{self, channel, Sender};
use futures::{SinkExt, StreamExt};
use log::error;

use crate::connection::{Event, Request};
use crate::handlers::HandleCommand;
use crate::index::{GetMailboxRequest, Mailbox, Permission};
use crate::server::{Command, ParseError, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct SelectHandler {
    mailbox_requests: Option<UnboundedSender<GetMailboxRequest>>,
}

impl SelectHandler {
    pub fn new(mailbox_requests: UnboundedSender<GetMailboxRequest>) -> Self {
        Self {
            mailbox_requests: Some(mailbox_requests),
        }
    }
}

#[async_trait::async_trait]
impl HandleCommand for SelectHandler {
    fn name<'a>(&self) -> &'a str {
        "SELECT"
    }
    async fn validate<'a>(&self, command: &'a Command) -> Result<()> {
        if command.command() != self.name() {
            return Ok(());
        }
        if command.num_args() < 1 {
            return Err(Box::new(ParseError {}));
        }
        Ok(())
    }
    async fn handle<'a>(&self, command: &'a Command) -> Result<Vec<Response>> {
        Ok(vec![
            Response::from("* 172 EXISTS").unwrap(),
            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
            Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
            Response::from("* LIST () \"/\" INBOX").unwrap(),
            Response::new(
                &command.tag(),
                ResponseStatus::OK,
                "[READ-WRITE] SELECT completed.",
            ),
        ])
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

            let (request_responder, reques_response_receiver): (
                Sender<Option<Mailbox>>,
                oneshot::Receiver<Option<Mailbox>>,
            ) = channel();
            if let Some(sender) = self.mailbox_requests.borrow_mut() {
                sender
                    .send(GetMailboxRequest {
                        name: folder.clone(),
                        responder: request_responder,
                        permission: Permission::ReadOnly,
                    })
                    .await?;
            }
            let mailbox = match reques_response_receiver.await {
                Ok(mailbox) => mailbox,
                Err(..) => {
                    error!(
                        "GetMailboxRequest response sender was dropped before sending a response"
                    );
                    None
                }
            };

            match mailbox {
                Some(mailbox) => {
                    request
                        .events
                        .send(Event::SELECT(PathBuf::from(folder.clone())))
                        .await?;
                    request
                        .responder
                        .send(vec![
                            Response::from(&format!("* {} EXISTS", &mailbox.count)).unwrap(),
                            Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
                            Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
                            Response::from(
                                "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)",
                            )
                            .unwrap(),
                            Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited")
                                .unwrap(),
                            Response::from(&format!("* LIST () \"/\" {}", folder)).unwrap(),
                            Response::new(
                                &request.command.tag(),
                                ResponseStatus::OK,
                                "[READ-WRITE] SELECT completed.",
                            ),
                        ])
                        .await?;
                }
                None => {
                    request
                        .responder
                        .send(vec![Response::new(
                            &request.command.tag(),
                            ResponseStatus::NO,
                            "No such mailbox",
                        )])
                        .await?;
                }
            }
        }
        if let Some(channel) = self.mailbox_requests.take() {
            drop(channel);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_std::path::PathBuf;
    use async_std::task::spawn;
    use futures::StreamExt;
    use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

    use super::SelectHandler;
    use crate::auth::User;
    use crate::connection::{Context, Event};
    use crate::handlers::tests::test_handle;
    use crate::handlers::HandleCommand;
    use crate::index::{GetMailboxRequest, Mailbox, Permission};
    use crate::server::{Command, Response, ResponseStatus};
    
    const EXISTING_MAILBOX: &str = "INBOX";
    
    async fn test_select<F, S>(command: Command, ctx: Option<Context>, assertions: F, events: Option<S>) where F: FnOnce(Vec<Response>), S: FnOnce(Event) {
        let (sender, mut receiver): (
            UnboundedSender<GetMailboxRequest>,
            UnboundedReceiver<GetMailboxRequest>,
        ) = unbounded();
        let index = spawn(async move {
            while let Some(request) = receiver.next().await {
                if request.name == EXISTING_MAILBOX {
                    request.responder.send(Some(Mailbox::new(EXISTING_MAILBOX, 172, vec![], Permission::ReadWrite))).unwrap();
                }
            }
        });
        let select_handler = SelectHandler::new(sender);
        
        test_handle(
            select_handler,
            command,
            assertions,
            events,
            ctx,
        )
        .await;
        index.await;
    }

    #[async_std::test]
    pub async fn test_can_select() {
        let (sender, _receiver): (
            UnboundedSender<GetMailboxRequest>,
            UnboundedReceiver<GetMailboxRequest>,
        ) = unbounded();
        let select_handler = SelectHandler::new(sender);
        let select_command = Command::new("a1", "SELECT", vec!["INBOX"]);
        let valid = select_handler.validate(&select_command).await;
        assert_eq!(valid.is_ok(), true);
        let response = select_handler.handle(&select_command).await;
        select_success(response.unwrap());
    }

    #[async_std::test]
    async fn test_select_handle() {
        let command = Command::new("a1", "SELECT", vec!["INBOX"]);

        let ctx = Context::of(Some(User::new("username", "password")), None);
        test_select(command, Some(ctx), 
            select_success,
            Some(|event| match event {
                Event::SELECT(folder) => {
                    assert_eq!(folder, PathBuf::from("INBOX"))
                }
                _ => {
                    panic!("SELECT command should only send SELECT events");
                }
            }),
        )
        .await;
    }

    #[async_std::test]
    async fn test_cannot_select_if_unauthenticated() {
        let command = Command::new("a1", "SELECT", vec!["INBOX"]);

        let mut f = Some(|_event| {});
        f.take();
        test_select(command, None, |response| {
            assert_eq!(response.len(), 1);
            assert_eq!(response[0], Response::new("a1", ResponseStatus::NO, "cannot SELECT when un-authenticated. Please authenticate using LOGIN or AUTHENTICATE."));
        }, f).await;
    }

    #[async_std::test]
    async fn test_select_bad_args() {
        let command = Command::new("a1", "SELECT", vec![]);
        let mut f = Some(|_event| {});
        f.take();
        test_select(
            command,
            None,
            |response| {
                assert_eq!(response.len(), 1);
                assert_eq!(
                    response[0],
                    Response::new("a1", ResponseStatus::BAD, "insufficient arguments")
                );
            },
            f,
        )
        .await;
    }

    fn select_success(response: Vec<Response>) {
        assert_eq!(
            response,
            vec!(
                Response::from("* 172 EXISTS").unwrap(),
                Response::from("* OK [UIDVALIDITY 3857529045] UIDs valid").unwrap(),
                Response::from("* OK [UIDNEXT 4392] Predicted next UID").unwrap(),
                Response::from("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)").unwrap(),
                Response::from("* OK [PERMANENTFLAGS (\\Deleted \\Seen \\*)] Limited").unwrap(),
                Response::from("* LIST () \"/\" INBOX").unwrap(),
                Response::new("a1", ResponseStatus::OK, "[READ-WRITE] SELECT completed.")
            )
        );
    }
}
