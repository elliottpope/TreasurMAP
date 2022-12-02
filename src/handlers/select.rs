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
use futures::{SinkExt, StreamExt};
use log::error;

use crate::connection::{Event, Request};
use crate::handlers::HandleCommand;
use crate::index::Mailbox;
use crate::mailbox::{self, MailboxRequest};
use crate::server::{Command, ParseError, Response, ResponseStatus};
use crate::util::{Receiver, Result};

use super::Handle;

pub struct SelectHandler {
    mailbox_requests: Option<UnboundedSender<mailbox::Request<Mailbox, MailboxRequest>>>,
}

impl SelectHandler {
    pub fn new(
        mailbox_requests: UnboundedSender<mailbox::Request<Mailbox, MailboxRequest>>,
    ) -> Self {
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

            let mailbox = if let Some(sender) = self.mailbox_requests.borrow_mut() {
                let (request, receiver) =
                    mailbox::Request::new(MailboxRequest::Get(folder.clone()));
                sender.send(request).await?;
                match receiver.await {
                    Ok(result) => match result {
                        Ok(mailbox) => Some(mailbox),
                        Err(..) => {
                            // TODO: parse errors for "insufficient permissions" and "mailbox does not exist"
                            error!(
                        "GetMailboxRequest response sender was dropped before sending a response"
                    );
                            None
                        }
                    },
                    Err(..) => None
                }
            } else {
                None
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
    use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
    use futures::channel::oneshot;

    use super::SelectHandler;
    use crate::auth::User;
    use crate::connection::{Context, Event};
    use crate::handlers::tests::test_handle;
    use crate::handlers::HandleCommand;
    use crate::index::{Mailbox, Permission};
    use crate::mailbox::{self, MailboxRequest, Request, RequestHandler};
    use crate::server::{Command, Response, ResponseStatus};
    use crate::util::Result;

    const EXISTING_MAILBOX: &str = "INBOX";

    struct TestMailboxes {
        receiver: Option<UnboundedReceiver<mailbox::Request<Mailbox, MailboxRequest>>>,
    }
    #[async_trait::async_trait]
    impl RequestHandler<Mailbox, MailboxRequest> for TestMailboxes {
        async fn handle(
            &mut self,
            data: MailboxRequest,
            responder: oneshot::Sender<Result<Mailbox>>,
        ) -> Result<()> {
            match data {
                MailboxRequest::Get(name) => {
                    if name == EXISTING_MAILBOX {
                        responder
                            .send(Ok(Mailbox::new(
                                EXISTING_MAILBOX,
                                172,
                                vec![],
                                Permission::ReadWrite,
                            )))
                            .unwrap();
                    }
                    Ok(())
                }
                _ => panic!("Cannot handle non Get requests"),
            }
        }
        fn incoming(&mut self) -> UnboundedReceiver<mailbox::Request<Mailbox, MailboxRequest>> {
            self.receiver.take().unwrap()
        }
    }

    async fn test_select<F, S>(
        command: Command,
        ctx: Option<Context>,
        assertions: F,
        events: Option<S>,
    ) where
        F: FnOnce(Vec<Response>),
        S: FnOnce(Event),
    {
        let (sender, receiver): (
            UnboundedSender<mailbox::Request<Mailbox, MailboxRequest>>,
            UnboundedReceiver<mailbox::Request<Mailbox, MailboxRequest>>,
        ) = unbounded();

        let index = spawn(async move {
            TestMailboxes {
                receiver: Some(receiver),
            }
            .start()
            .await
        });
        let select_handler = SelectHandler::new(sender);

        test_handle(select_handler, command, assertions, events, ctx).await;
        index.await.unwrap();
    }

    #[async_std::test]
    pub async fn test_can_select() {
        let (sender, _receiver): (
            UnboundedSender<Request<Mailbox, MailboxRequest>>,
            UnboundedReceiver<Request<Mailbox, MailboxRequest>>,
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
        test_select(
            command,
            Some(ctx),
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
