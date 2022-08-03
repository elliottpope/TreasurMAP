use imap;
use imaprust::server::{Configuration, DefaultServer};
use imaprust::handlers::{DelegatingCommandHandler, login::LoginHandler};

use std::net::TcpStream;

use async_std::task;

#[test]
fn test_can_connect() {
    let (sender, receiver): (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>) = std::sync::mpsc::channel();

    let config = Configuration::default(); // TODO: add the new, from_env, and from_file options to override configs
    let delegating_handler = DelegatingCommandHandler::new();
    task::block_on(delegating_handler.register_command(LoginHandler{}));
    let server = DefaultServer::new(config, delegating_handler);
    let server_handle = std::thread::spawn(move || {
        task::block_on(server.start_with_notification(sender))
    });

    receiver.recv().unwrap();

    let connection = TcpStream::connect("127.0.0.1:3143").unwrap();
    let mut client = imap::Client::new(connection);
    client.read_greeting().unwrap();
    client.debug = true;

    // the client we have here is unauthenticated.
    // to do anything useful with the e-mails, we need to log in
    let mut imap_session = client
        .login("me@example.com", "password")
        .map_err(|e| e.0)
        .unwrap();

    // we want to fetch the first email in the INBOX mailbox
    imap_session.select("INBOX").unwrap();

    // fetch message number 1 in this mailbox, along with its RFC822 field.
    // RFC 822 dictates the format of the body of e-mails
    let messages = imap_session.fetch("1", "RFC822").unwrap();
    let message = messages.iter().next().unwrap();

    // extract the message's body
    let body = message.body().expect("message did not have a body!");
    let body = std::str::from_utf8(body)
        .expect("message was not valid utf-8")
        .to_string();

    assert_eq!(body, "This is a test email body.");

    // be nice to the server and log out
    imap_session.logout().unwrap();
    drop(server_handle)
}
