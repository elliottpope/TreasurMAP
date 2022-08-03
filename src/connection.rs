use std::{
    collections::hash_map::{Entry, HashMap},
    sync::Arc,
};

use futures::{channel::mpsc, select, FutureExt, SinkExt};

use async_std::{io::{BufReader, Write}, net::TcpStream, prelude::*, task, task::JoinHandle};

use async_listen::backpressure::Token;

use async_lock::RwLock;

use log::error;

use crate::util::Result;
use crate::util::{Receiver, Sender};

#[derive(Debug)]
pub enum Void {}

pub async fn connection_loop(
    _token: Token,
    mut broker: Sender<Event>,
    stream: TcpStream,
) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    let name = match lines.next().await {
        None => return Err("peer disconnected immediately".into()),
        Some(line) => line?,
    };
    let (_shutdown_sender, shutdown_receiver) = mpsc::unbounded::<Void>();
    broker
        .send(Event::NewPeer {
            name: name.clone(),
            stream: Arc::clone(&stream),
            shutdown: shutdown_receiver,
        })
        .await
        .unwrap();

    while let Some(line) = lines.next().await {
        let line = line?;
        let (dest, msg) = match line.find(':') {
            None => continue,
            Some(idx) => (&line[..idx], line[idx + 1..].trim()),
        };
        let dest: Vec<String> = dest
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();
        let msg: String = msg.trim().to_string();

        broker
            .send(Event::Message {
                from: name.clone(),
                to: dest,
                msg,
            })
            .await
            .unwrap();
    }

    Ok(())
}

async fn connection_writer_loop(
    messages: &mut Receiver<String>,
    stream: Arc<TcpStream>,
    mut shutdown: Receiver<Void>,
) -> Result<()> {
    let mut stream = &*stream;
    loop {
        select! {
            msg = messages.next().fuse() => match msg {
                Some(msg) => stream.write_all(msg.as_bytes()).await?,
                None => break,
            },
            void = shutdown.next().fuse() => match void {
                Some(void) => match void {},
                None => break,
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum Event {
    NewPeer {
        name: String,
        stream: Arc<TcpStream>,
        shutdown: Receiver<Void>,
    },
    Message {
        from: String,
        to: Vec<String>,
        msg: String,
    },
}

pub async fn broker_loop(mut events: Receiver<Event>) -> Result<()> {
    let (disconnect_sender, mut disconnect_receiver) =
        mpsc::unbounded::<(String, Receiver<String>)>();
    let mut peers: HashMap<String, Sender<String>> = HashMap::new();

    loop {
        let event = select! {
            event = events.next().fuse() => match event {
                None => break,
                Some(event) => event,
            },
            disconnect = disconnect_receiver.next().fuse() => {
                let (name, _pending_messages) = disconnect.unwrap();
                assert!(peers.remove(&name).is_some());
                continue;
            },
        };
        match event {
            Event::Message { from, to, msg } => {
                for addr in to {
                    if let Some(peer) = peers.get_mut(&addr) {
                        let msg = format!("from {}: {}\n", from, msg);
                        peer.send(msg).await.unwrap();
                    }
                }
            }
            Event::NewPeer {
                name,
                stream,
                shutdown,
            } => match peers.entry(name.clone()) {
                Entry::Occupied(..) => (),
                Entry::Vacant(entry) => {
                    let (client_sender, mut client_receiver) = mpsc::unbounded();
                    entry.insert(client_sender);
                    let mut disconnect_sender = disconnect_sender.clone();
                    spawn_and_log_error(async move {
                        let res =
                            connection_writer_loop(&mut client_receiver, stream, shutdown).await;
                        disconnect_sender
                            .send((name, client_receiver))
                            .await
                            .unwrap();
                        res
                    });
                }
            },
        }
    }
    drop(peers);
    drop(disconnect_sender);
    while let Some((_name, _pending_messages)) = disconnect_receiver.next().await {}
    Ok(())
}

fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            error!("{}", e)
        }
    })
}

#[async_trait::async_trait]
trait ManageConnections {
    async fn start(&mut self) -> Sender<Event>;
    async fn stop(&mut self, event_loop_sender: Sender<Event>);
}

struct ConnectionManager<W> where W: Write {
    broker: Option<JoinHandle<()>>,
    clients: Arc<RwLock<HashMap<String, Arc<W>>>>,
}

#[async_trait::async_trait]
impl ManageConnections for ConnectionManager<TcpStream> {
    async fn start(&mut self) -> Sender<Event> {
        self.clients = Arc::new(RwLock::new(HashMap::new()));
        let (sender, mut receiver) = mpsc::unbounded();
        let event_loop_clients_reference = self.clients.clone();
        let event_loop = task::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Event::NewPeer { name, stream, .. } => {
                        let mut lock = event_loop_clients_reference.write().await;
                        (*lock).insert(name, stream);
                    },
                    Event::Message { .. } => {}
                }
            }
        });
        let _unused = self.broker.insert(event_loop);
        sender
    }
    async fn stop(&mut self, event_loop_sender: Sender<Event>) {
        drop(event_loop_sender);
        if let Some(task) = &mut self.broker {
            task.await
        }
    }
}
