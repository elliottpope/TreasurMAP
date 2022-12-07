use imap;
use imaprust::{server::ServerBuilder, auth::inmemory::InMemoryUserStore};

use std::net::TcpStream;

use async_std::task;

#[test]
fn test_can_connect() {
    let (sender, receiver): (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>) = std::sync::mpsc::channel();

    let server_handle = std::thread::spawn(move || {
        task::block_on(async {
    let user_store = InMemoryUserStore::new().with_user("me@example.com", "password");
            
            let builder = ServerBuilder::new().with_user_store(user_store);
            let server = builder.bind().await.unwrap();
            sender.send(()).unwrap();
            server.listen().await
        })
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
    let messages = imap_session.fetch("1", "BODY[TEXT]").unwrap();
    let message = messages.iter().next().unwrap();

    // extract the message's body
    let body = message.text().expect("message did not have a body!");
    let body = std::str::from_utf8(body)
        .expect("message was not valid utf-8")
        .to_string();

    assert_eq!(body, "This is a test email body.");

    // be nice to the server and log out
    imap_session.logout().unwrap();
    drop(server_handle)
}
