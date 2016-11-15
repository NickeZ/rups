use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use std::error::{Error};
use mio::*;
use mio::tcp::{TcpListener};

use telnet_client::TelnetClient;
use history::History;

type Slab<T> = ::slab::Slab<T, Token>;

#[derive(PartialEq, Copy, Clone)]
pub enum BindKind {
    Control,
    Log,
}

pub struct TelnetServer {
    sockets: HashMap<Token, (TcpListener, BindKind)>,
    clients: Slab<TelnetClient>,
    token_counter: usize,
}

impl TelnetServer {
    pub fn new() -> TelnetServer {
        TelnetServer {
            sockets: HashMap::new(),
            clients: Slab::with_capacity(1024),
            token_counter: ::SERVER_BIND_START.0,
        }
    }

    pub fn add_bind(&mut self, poll: &Poll, addr:SocketAddr, kind:BindKind) {
        if let Ok(listener) = TcpListener::bind(&addr) {
            let tok = Token(self.token_counter);
            poll.register(&listener, tok,
                          Ready::readable(), PollOpt::edge()).unwrap();
            self.sockets.insert(tok, (listener, kind));
            self.token_counter += 1;

        } else {
            panic!("Failed to bind to port {}", addr);
        }
    }

    /// Try to accept a connection, will return false if token is not a bind socket.
    pub fn try_accept(&mut self, poll:&Poll, token:Token, history:Rc<RefCell<History>>) -> bool{
        if self.sockets.contains_key(&token) {
            let (ref socket, ref kind) = self.sockets[&token];
            let (client_stream, client_addr) = match socket.accept() {
                Err(why) => {
                    println!("Failed to accept connection: {}", why.description());
                    return false;
                },
                Ok((stream, addr)) => {
                    ::push_info(&history, format!("[{}] Connection established\n", addr));
                    self.poll_clients_write(&poll);
                    (stream, addr)
                },
            };
            // Insert new client into client collection
            let client = TelnetClient::new(client_stream, client_addr, history, *kind);
            if let Ok(new_token) = self.clients.insert(client) {
                let client = &self.clients[new_token];
                poll.register(client.get_stream(), new_token, client.interest,
                                PollOpt::edge() | PollOpt::oneshot()).unwrap();
            };
            return true;
        }
        false
    }

    pub fn conn<'a>(&'a mut self, tok:Token) -> &'a mut TelnetClient {
        &mut self.clients[tok]
    }

    pub fn remove(&mut self, tok:Token) -> Option<TelnetClient> {
        self.clients.remove(tok)
    }

    /*
    pub fn socket(&mut self, tok:Token) -> Option<&TcpListener> {
        self.sockets.get(&tok)
    }
    */

    pub fn poll_clients_write(&self, poll:& Poll){
        for (tok, client) in self.clients.into_iter().enumerate() {
            poll.reregister(client.get_stream(), Token(tok), Ready::writable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }
    }
}

