use futures::{
    channel::{
        mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
        oneshot::{self, channel},
    },
    SinkExt, StreamExt,
};

use crate::{index::Mailbox, util::Result};

pub struct Request<T, S> {
    data: S,
    responder: oneshot::Sender<Result<T>>,
}
impl<T, S> Request<T, S> {
    pub fn new(request: S) -> (Self, oneshot::Receiver<Result<T>>) {
        let (sender, receiver): (oneshot::Sender<Result<T>>, oneshot::Receiver<Result<T>>) = channel();
        (Self {
            data: request,
            responder: sender,
        }, receiver)
    }
}
#[async_trait::async_trait]
pub trait RequestHandler<T: Send + Sync, S: Send + Sync> {
    async fn handle(&mut self, data: S, responder: oneshot::Sender<Result<T>>) -> Result<()>;
    fn incoming(&mut self) -> UnboundedReceiver<Request<T, S>>;
    async fn start(&mut self) -> Result<()> {
        let mut receiver = self.incoming();
        while let Some(request) = receiver.next().await {
            let data = request.data;
            let responder = request.responder;
            self.handle(data, responder).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum IndexRequest {
    Mailbox(String),
    Message(String, String, String),
}

#[derive(Debug, Clone)]
pub enum MailboxRequest {
    Get(String),
    Add(Mailbox),
}

pub struct Mailboxes {
    index: UnboundedSender<IndexRequest>,
    requests: 
        UnboundedSender<Request<Mailbox, MailboxRequest>>,
    receiver: Option<UnboundedReceiver<Request<Mailbox, MailboxRequest>>>,
}

impl Mailboxes {
    pub fn new(index: UnboundedSender<IndexRequest>) -> Self {
        let (requests, receiver): (UnboundedSender<Request<Mailbox, MailboxRequest>>, UnboundedReceiver<Request<Mailbox, MailboxRequest>>) = unbounded();
        Self {
            index,
            requests,
            receiver: Some(receiver),
        }
    }
    pub fn requests(&self) -> UnboundedSender<Request<Mailbox, MailboxRequest>> {
        self.requests.clone()
    }
}

#[async_trait::async_trait]
impl RequestHandler<Mailbox, MailboxRequest> for Mailboxes {
    async fn handle(&mut self, data: MailboxRequest, responder: oneshot::Sender<Result<Mailbox>>) -> Result<()> {
        match data {
            MailboxRequest::Get(mailbox) => {
                self.index.send(IndexRequest::Mailbox(mailbox)).await?;
            }
            _ => {}
        }
        Ok(())
    }
    fn incoming(&mut self) -> UnboundedReceiver<Request<Mailbox, MailboxRequest>> {
        self.receiver.take().unwrap()
    }
}
