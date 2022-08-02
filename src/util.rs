use std::error::Error;
use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub type Sender<T> = UnboundedSender<T>;
pub type Receiver<T> = UnboundedReceiver<T>;