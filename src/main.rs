mod server;
mod parser;
mod handler;

use std::boxed::Box;
use std::panic::catch_unwind;
use std::time::Duration;
use simple_logger::SimpleLogger;
use log::LevelFilter::Info;

use server::{IMAPServer, ServerConfiguration};
use crate::handler::RequestHandler;
use crate::parser::Parser;



fn main() {
    SimpleLogger::new().with_level(Info).env().init().unwrap();

    let config = ServerConfiguration::new(1433, 10, Duration::MAX);
    let mut server = IMAPServer::new(config);
    let handler = Box::leak(Box::new(RequestHandler::new(1024, Parser::new())));
    server.start(handler);
    match catch_unwind(|| {loop {}}) {
        Ok(_) => {},
        Err(_) => server.stop()
    }
}
