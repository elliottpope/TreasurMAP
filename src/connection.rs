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
use crate::handlers::HandleCommand;

pub struct Connection {
    writer: Option<JoinHandle<()>>,
    stream: Arc<TcpStream>,
    responder: Sender<Vec<Response>>,
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
                "IMAP4rev2 server ready".to_string(),
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

    pub async fn handle<T:HandleCommand>(&mut self, handler: Arc<T>) -> Result<()> {
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
            let response = handler.handle(&command).await?;
            self.responder.send(response).await.unwrap();
        }
        drop(&self.responder);
        if let Some(writer) = self.writer.take() {
            writer.await
        }
        Ok(())
    }
}
