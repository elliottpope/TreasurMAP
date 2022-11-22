use std::collections::HashMap;
use std::sync::Arc;

use async_lock::RwLock;
use async_std::path::PathBuf;
use async_std::{
    io::BufReader,
    net::TcpStream,
    prelude::*,
    task::spawn,
    task::JoinHandle,
};

use futures::channel::oneshot::{self, channel};
use futures::{SinkExt, channel::mpsc::unbounded};
use log::{info, trace};

use crate::auth::User;
use crate::server::{Command, Response, ResponseStatus};
use crate::util::{Result, Receiver, Sender};

pub struct Connection {
    shutdown: oneshot::Receiver<()>,
    state_manager: Option<JoinHandle<()>>,
    state_updater: Sender<Event>,
    state: Arc<RwLock<Context>>,
    writer: Option<JoinHandle<()>>,
    stream: Arc<TcpStream>,
    responder: Sender<Vec<Response>>,
}

#[derive(Debug, Clone, Default)]
pub struct Context{
    current_folder: Option<PathBuf>,
    user: Option<User>,
}

#[derive(Debug, Clone)]
pub enum Event {
    AUTH(User),
    SELECT(PathBuf),
    UNAUTH(),
}

impl Context {
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }
    pub fn is_selected(&self) -> bool {
        self.current_folder.is_some()
    }
    pub fn of(user: Option<User>, folder: Option<PathBuf>) -> Self {
        Self { current_folder: folder, user }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub command: Command,
    pub responder: Sender<Vec<Response>>,
    pub events: Sender<Event>,
    pub context: Context,
}

impl Connection {
    pub async fn new(stream: TcpStream) -> Result<Self> {
        let stream = Arc::new(stream);
        let output = Arc::clone(&stream);
        let (mut response_sender, mut response_receiver): (
            Sender<Vec<Response>>,
            Receiver<Vec<Response>>,
        ) = unbounded();
        let context = Arc::new(RwLock::new(Context::default()));
        let ctx = context.clone();
        let (event_sender, mut event_receiver): (Sender<Event>, Receiver<Event>) = unbounded();
        let (shutdown_signal, shutdown): (oneshot::Sender<()>, oneshot::Receiver<()>) = channel();
        trace!(
            "Spawning writer thread for connection from {}",
            &stream.peer_addr()?
        );
        let state_manager = spawn(async move {
            while let Some(event) = event_receiver.next().await {
                match event {
                    Event::AUTH(user) => {
                        let mut lock = ctx.write().await;
                        lock.user.replace(user);
                        drop(lock);
                    },
                    Event::SELECT(folder) => {
                        let mut lock = ctx.write().await;
                        lock.current_folder.replace(folder);
                        drop(lock);
                    }
                    Event::UNAUTH() => {
                        let mut lock = ctx.write().await;
                        lock.current_folder.take();
                        lock.user.take();
                        drop(lock);
                        break;
                    }
                }
            }
            shutdown_signal.send(()).unwrap();
        });
        let writer = spawn(async move {
            let mut output = &*output;
            while let Some(response) = response_receiver.next().await {
                for reply in response {
                    trace!(
                        "Sending {} to client at {}",
                        &reply.to_string(),
                        &output.peer_addr().unwrap()
                    );
                    output.write(reply.to_string().as_bytes()).await.unwrap();
                    output.write("\r\n".as_bytes()).await.unwrap();
                }
            }
        });
        info!("Sending greeting to client at {}", &stream.peer_addr()?);
        response_sender
            .send(vec![Response::new(
                "*",
                ResponseStatus::OK,
                "IMAP4rev2 server ready",
            )])
            .await?;
        trace!(
            "Reading input from connection at {}",
            &stream.peer_addr().unwrap()
        );
        Ok(Connection {
            state_manager: Some(state_manager),
            state_updater: event_sender,
            state: context,
            writer: Some(writer),
            stream,
            responder: response_sender,
            shutdown,
        })
    }

    pub async fn handle(&mut self, handler: &HashMap<String, Sender<Request>>) -> Result<()> {
        let input = BufReader::new(&*self.stream);
        let mut lines = input.lines();
        while let Some(line) = lines.next().await {
            let shutdown = match self.shutdown.try_recv() {
                Ok(signal) => {
                    match signal {
                        Some(..) => true,
                        None => false,
                    }
                },
                Err(..) => {
                    true
                }
            };
            if shutdown {
                break;
            }
            let line = line?;
            trace!(
                "Read {} from client at {}",
                &line,
                &self.stream.peer_addr().unwrap()
            );
            let command = Command::parse(&line)?;
            if let Some(mut channel) = handler.get(&command.command()) {
                let ctx = self.state.read().await;
                channel.send(Request{command, responder: self.responder.clone(), context: ctx.clone(), events: self.state_updater.clone()}).await?;
                drop(ctx);
            };
        }
        drop(&self.responder);
        if let Some(writer) = self.writer.take() {
            writer.await
        }
        drop(&self.state_updater);
        if let Some(updater) = self.state_manager.take() {
            updater.await
        }
        Ok(())
    }
}
