use async_std::task;
use imaprust::util::Result;
use imaprust::server::{Server, DefaultServer, Configuration};

pub(crate) fn main() -> Result<()> {
    let config = Configuration::default(); // TODO: add the new, from_env, and from_file options to override configs
    let server = DefaultServer::new(config);
    task::block_on(server.start())
}