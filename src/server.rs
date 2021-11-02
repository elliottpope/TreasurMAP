use crate::handler::HandleRequest;
use log::{debug, error, info, trace, warn};
use std::convert::TryInto;
use std::io::ErrorKind::WouldBlock;
use std::net::TcpListener;
use std::ops::{Add, Neg};
use std::panic::panic_any;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;
use threadpool::ThreadPool;

pub struct ServerConfiguration {
    port: i16,
    handler_threads: usize,
    stale_connection_timeout: Duration,
}

impl ServerConfiguration {
    pub fn new(
        port: i16,
        handler_threads: usize,
        stale_connection_timeout: Duration,
    ) -> ServerConfiguration {
        return ServerConfiguration {
            port: port,
            handler_threads: handler_threads,
            stale_connection_timeout: stale_connection_timeout,
        };
    }
}

pub struct IMAPServer {
    _config: ServerConfiguration,
    _listener: Option<TcpListener>,
    _executor: Arc<Mutex<ThreadPool>>,
    _exit_channel: (Sender<()>, Arc<Mutex<Receiver<()>>>),
    _t: Option<JoinHandle<()>>,
}

impl IMAPServer {
    pub fn new(configuration: ServerConfiguration) -> IMAPServer {
        let channel = channel();
        let num_threads = configuration.handler_threads;
        IMAPServer {
            _config: configuration,
            _listener: None,
            _executor: Arc::new(Mutex::new(ThreadPool::with_name(
                String::from("IMAPRequestExecutor"),
                num_threads,
            ))),
            _exit_channel: (channel.0, Arc::new(Mutex::new(channel.1))),
            _t: None,
        }
    }
    pub fn start<T: HandleRequest + Send + Sync>(&mut self, handler: &'static T) {
        // Create listener on port
        info!("Starting server at {}:{}", "127.0.0.1", self._config.port);
        let listener = match TcpListener::bind(format!("{}:{}", "127.0.0.1", self._config.port)) {
            Ok(result) => {
                result
                    .set_nonblocking(true)
                    .expect("TcpListener cannot be set to non-blocking");
                result
            }
            Err(e) => panic_any(e),
        };
        info!("TcpListener listening on port {}", self._config.port);
        // Store the reference to the listener
        self._listener.as_ref().replace(&listener);

        // Create channel for exiting the listener loop
        let rx = self._exit_channel.1.clone();
        let pool = self._executor.clone();
        let staleness_timeout: i64 = self
            ._config
            .stale_connection_timeout
            .as_millis()
            .try_into()
            .expect("Provided staleness timeout cannot be converted to i64. It is too large");
        let t = spawn(move || {
            for stream in listener.incoming() {
                match rx
                    .try_lock()
                    .expect("Failed to acquire lock on receiver")
                    .try_recv()
                {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        debug!("IMAPServer listener has received disconnect command");
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }
                match stream {
                    Ok(stream) => {
                        let peer_addr = stream.peer_addr().unwrap();
                        info!(
                            "Accepted connection from {} at {}",
                            peer_addr,
                            chrono::Local::now()
                        );
                        stream
                            .set_nonblocking(true)
                            .expect("Failed to set TcpStream as nonblocking");
                        pool.try_lock()
                            .expect("Could not acquire lock on thread pool to execute command")
                            .execute(move || {
                                let mut last_read = chrono::Utc::now();
                                loop {
                                    let stale_check_duration =
                                        chrono::Duration::milliseconds(staleness_timeout);
                                    let earliest_non_stale_read =
                                        chrono::Utc::now().add(stale_check_duration.neg());
                                    if last_read.lt(&earliest_non_stale_read) {
                                        info!(
                                            "Connection to peer {} is stale. Last read was at {}, time is now {}",
                                            peer_addr, last_read, chrono::Local::now()
                                        );
                                        info!("Terminating connection to {}", peer_addr);
                                        break;
                                    };
                                    match handler.handle(&stream, &stream) {
                                        Ok(..) => {
                                            last_read = chrono::Utc::now();
                                            trace!("Last read updated to {}", last_read);
                                        }
                                        Err(e) if e.can_ignore() => {}
                                        Err(_) => {
                                            info!("Terminating connection to {}", peer_addr);
                                            break;
                                        }
                                    };
                                }
                                drop(stream);
                                debug!("TcpStream on port {} has been dropped", peer_addr);
                            });
                        let active_threads = pool
                            .try_lock()
                            .expect("Could not acquire lock on thread pool to execute command")
                            .active_count();
                        trace!("Thread pool is running {} threads", active_threads);
                    }
                    Err(ref e) if e.kind() == WouldBlock => {
                        sleep(Duration::from_millis(1000));
                        continue;
                    }
                    Err(e) => {
                        error!(
                            "Error {} occured while trying to establish connection to peer",
                            e
                        );
                        continue;
                    }
                };
            }
        });
        self._t = Option::from(t);
    }
    pub fn stop(mut self) {
        info!(
            "Shutting down server listening at {}:{}",
            "127.0.0.1", self._config.port
        );
        self._exit_channel
            .0
            .send(())
            .expect("Could not send termination message to listener thread");
        match self._t.take().map(JoinHandle::join) {
            Some(thread) => match thread {
                Ok(..) => debug!("Listener thread exited successfully"),
                Err(..) => {
                    error!("Listener thread did not exit successfully. Possible resource leak")
                }
            },
            None => warn!("Listener thread has not been set. Cannot terminate listener thread"),
        };
        if self._listener.is_some() {
            debug!("Dropping the server listener");
            drop(self._listener.as_ref().unwrap());
        }
        info!(
            "Server listening at {}:{} has been shutdown successfully",
            "127.0.0.1", self._config.port
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{IMAPServer, ServerConfiguration};
    use crate::handler::HandleRequest;
    use crate::parser::IMAPError;
    use std::io::{BufRead, BufReader, Error, ErrorKind, Read, Write};
    use std::net::TcpStream;
    use std::panic::panic_any;
    use std::thread::{sleep, spawn};
    use std::time::Duration;
    use std::ops::{Add, Neg};
    use log::debug;

    #[test]
    fn test_can_listen_on_port() {
        struct EmptyRequestHandler {}
        impl HandleRequest for EmptyRequestHandler {
            fn handle<R: Read, W: Write>(&self, _input: R, _output: W) -> Result<(), IMAPError> {
                Ok(())
            }
        }

        run_with_server(3133, &EmptyRequestHandler {}, |port: i16| {
            let thread =
                spawn(
                    move || match TcpStream::connect(format!("{}:{}", "127.0.0.1", port)) {
                        Ok(..) => {}
                        Err(e) => {
                            eprintln!("{:?}", e);
                            panic_any(e)
                        }
                    },
                );

            match thread.join() {
                Ok(..) => {}
                Err(..) => {
                    panic!("An error occured while writing to the server")
                }
            }
        });
    }
    #[test]
    fn test_connection_closes_after_timeout() {
        run_with_server(3136, &EchoingRequestHandler {}, |port: i16| {
            let thread = spawn(move || {
                let stream = match TcpStream::connect(format!("{}:{}", "127.0.0.1", port)) {
                    Ok(mut client) => {
                        client.write(b"This is a test message\n").expect("Was not able to write to stream");
                        client.flush().expect("Failed to flush write buffer");
                        let mut buf: [u8; 23] = [0;23];
                        client.read(&mut buf).expect("Unable to read from stream");
                        assert_eq!(b"This is a test message\n", &buf);
                        client
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        panic_any(e)
                    }
                };
                sleep(Duration::from_secs(1));
                let mut buf: [u8; 1] = [0; 1];
                match stream.peek(&mut buf) {
                    Ok(peeked) => {
                        debug!("Received byte {:02x?}", buf);
                        if peeked > 0 {
                            debug!("{} bytes read from closed connection", peeked);
                            panic!("This peek should have failed");
                        }
                        debug!("Zero bytes returned after peek")
                    }
                    Err(e) => {
                        eprintln!("{:?}", e)
                    }
                };
            });
            match thread.join() {
                Ok(..) => {}
                Err(..) => {
                    panic!("An error occured while writing to the server")
                }
            }
        })
    }
    #[test]
    fn test_can_read_data_below_limit() {
        run_with_server(3134, &EchoingRequestHandler {}, |port: i16| {
            let thread = std::thread::spawn(move || {
                match TcpStream::connect(format!("{}:{}", "127.0.0.1", port)) {
                    Ok(mut client) => {
                        let mut buf: [u8; 14] = [0; 14];
                        client.write(b"This is a test\n").unwrap();
                        client.read(&mut buf).unwrap();
                        assert_eq!(b"This is a test", &buf);
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        panic_any(e)
                    }
                }
            });

            match thread.join() {
                Ok(..) => {}
                Err(..) => {
                    panic!("An error occured while writing to the server")
                }
            }
        });
    }
    #[test]
    fn test_can_handle_multiple_connections() {
        run_with_server(3135, &EchoingRequestHandler {}, |port: i16| {
            let thread1 = std::thread::spawn(move || {
                match TcpStream::connect(format!("{}:{}", "127.0.0.1", port)) {
                    Ok(mut client) => {
                        let mut buf: [u8; 14] = [0; 14];
                        client.write(b"This is test 1\n").unwrap();
                        client.read(&mut buf).unwrap();
                        assert_eq!(b"This is test 1", &buf);
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        panic_any(e)
                    }
                }
            });
            let thread2 = std::thread::spawn(move || {
                match TcpStream::connect(format!("{}:{}", "127.0.0.1", port)) {
                    Ok(mut client) => {
                        let mut buf: [u8; 14] = [0; 14];
                        client.write(b"This is test 2\n").unwrap();
                        client.read(&mut buf).unwrap();
                        assert_eq!(b"This is test 2", &buf);
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        panic_any(e)
                    }
                }
            });

            match thread1.join() {
                Ok(..) => {}
                Err(..) => {
                    panic!("An error occured while writing to the server")
                }
            }
            match thread2.join() {
                Ok(..) => {}
                Err(..) => {
                    panic!("An error occured while writing to the server")
                }
            }
        })
    }

    #[test]
    fn test_difference_of_time() {
        let now = chrono::Local::now();
        sleep(Duration::from_millis(500));
        let after_wait = chrono::Local::now();
        let a_little_before = after_wait.add(chrono::Duration::milliseconds(400).neg());
        assert_eq!(now.lt(&a_little_before), true)
    }

    fn run_with_server<H: HandleRequest + Send + Sync>(
        port: i16,
        handler: &'static H,
        test: fn(port: i16),
    ) {
        let config = ServerConfiguration {
            port: port,
            handler_threads: 2,
            stale_connection_timeout: Duration::from_millis(500),
        };
        let mut server = IMAPServer::new(config);
        server.start(handler);

        test(port);

        server.stop();
    }

    struct EchoingRequestHandler {}
    impl HandleRequest for EchoingRequestHandler {
        fn handle<R: Read, W: Write>(&self, mut input: R, mut output: W) -> Result<(), IMAPError> {
            let mut buf: Vec<u8> = Vec::with_capacity(64);

            match BufReader::new(input.by_ref())
                .take(64)
                .read_until(b'\n', &mut buf)
            {
                Ok(read) => {
                    debug!(
                        "Read {} bytes from stream at {}",
                        read,
                        chrono::Local::now()
                    );
                    if read == 0 {
                        return Err(IMAPError::new(Vec::new(),
                    Error::new(ErrorKind::ConnectionAborted, "Empty read indicates stream has been closed by client. Server side stream must also be closed.")
                    ));
                    }
                }
                Err(e) => {
                    let wrapper = IMAPError::new(Vec::new(), e);
                    if wrapper.can_ignore() {
                        return Err(wrapper);
                    }
                    return Err(wrapper);
                }
            };
            match output.write(&mut buf) {
                Ok(written) => {
                    debug!(
                        "Wrote {} bytes to stream at {}",
                        written,
                        chrono::Local::now()
                    )
                }
                Err(e) => {
                    eprintln!("{:?}", e)
                }
            };
            output.flush().expect("Failed to flush output buffer");
            Ok(())
        }
    }
}
