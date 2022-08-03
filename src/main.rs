use async_std::task;
use imaprust::util::Result;
use imaprust::server::Server;

pub(crate) fn main() -> Result<()> {
    let server = Server::default();
    task::block_on(server.start())
}