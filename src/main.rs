use async_std::task;
use imaprust::util::Result;
use imaprust::server::ServerBuilder;

pub(crate) fn main() -> Result<()> {
    task::block_on(ServerBuilder::new().listen())
}