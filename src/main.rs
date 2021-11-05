mod auth;
mod error;
mod parser;
mod server;
mod requests;
mod handlers;

use log::LevelFilter::Info;
use simple_logger::SimpleLogger;
use std::boxed::Box;
use std::convert::TryInto;
use std::panic::catch_unwind;
use std::time::Duration;

use crate::requests::{RequestHandlerBuilder};
use crate::parser::Parser;
use crate::auth::InMemoryUserStore;
use crate::server::{IMAPServer, ServerConfiguration};

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
    let request_handler = RequestHandlerBuilder::new()
        .with_buffer_size(1024)
        .with_parser(Parser::new())
        .with_user_store(InMemoryUserStore::new().with_user("test".to_string(), "test".to_string()))
        .build();
    let boxed_handler = Box::leak(Box::new(request_handler));
    server.start(boxed_handler);
    match catch_unwind(|| loop {}) {
        Ok(_) => {}
        Err(_) => server.stop(),
    }
}
