mod auth;
mod error;
mod handler;
mod parser;
mod server;

use log::LevelFilter::Info;
use simple_logger::SimpleLogger;
use std::boxed::Box;
use std::convert::TryInto;
use std::panic::catch_unwind;
use std::time::Duration;

use crate::handler::RequestHandler;
use crate::parser::Parser;
use server::{IMAPServer, ServerConfiguration};

fn main() {
    SimpleLogger::new().with_level(Info).env().init().unwrap();

    let config = ServerConfiguration::new(
        1433,
        10,
        Duration::from_nanos(
            i64::MAX
                .try_into()
                .expect("Failed to coerce i64::MAX to u64"),
        ),
    );
    let mut server = IMAPServer::new(config);
    let handler = Box::leak(Box::new(RequestHandler::new(1024, Parser::new())));
    server.start(handler);
    match catch_unwind(|| loop {}) {
        Ok(_) => {}
        Err(_) => server.stop(),
    }
}
