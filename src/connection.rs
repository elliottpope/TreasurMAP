use std::collections::HashMap;
use std::sync::Arc;

use async_std::{
    io::BufReader,
    net::TcpStream,
    prelude::*,
    task::spawn,
    task::JoinHandle,
};

use futures::{SinkExt, channel::mpsc::unbounded};
use log::{info, trace};

use crate::server::{Command, Response, ResponseStatus};
use crate::util::{Result, Receiver, Sender};

pub struct Connection {
    writer: Option<JoinHandle<()>>,
    stream: Arc<TcpStream>,
    responder: Sender<Vec<Response>>,
}

#[derive(Debug, Clone)]
pub struct Context{}

#[derive(Debug, Clone)]
pub struct Request {
    pub command: Command,
    pub responder: Sender<Vec<Response>>,
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
        trace!(
            "Spawning writer thread for connection from {}",
            &stream.peer_addr()?
        );
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
                "*".to_string(),
                ResponseStatus::OK,
                "IMAP4rev2 server ready",
            )])
            .await?;
        trace!(
            "Reading input from connection at {}",
            &stream.peer_addr().unwrap()
        );
        Ok(Connection {
            writer: Some(writer),
            stream,
            responder: response_sender,
        })
    }

    pub async fn handle(&mut self, handler: &HashMap<String, Sender<Request>>) -> Result<()> {
        let input = BufReader::new(&*self.stream);
        let mut lines = input.lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            trace!(
                "Read {} from client at {}",
                &line,
                &self.stream.peer_addr().unwrap()
            );
            let command = Command::parse(&line)?;
            if let Some(mut channel) = handler.get(&command.command()) {
                channel.send(Request{command, responder: self.responder.clone(), context: Context{}}).await?;
            };
        }
        drop(&self.responder);
        if let Some(writer) = self.writer.take() {
            writer.await
        }
        Ok(())
    }
}
