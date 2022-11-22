use async_std::task;
use imaprust::util::Result;
use imaprust::server::Server;

pub(crate) fn main() -> Result<()> {
    let mut server = Server::default();
    task::block_on(server.start())
}