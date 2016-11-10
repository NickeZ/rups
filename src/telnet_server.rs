use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use std::error::{Error};
use mio::*;
use mio::tcp::{TcpListener, TcpStream};

use telnet_client::TelnetClient;
use history::History;

type Slab<T> = ::slab::Slab<T, Token>;

pub struct TelnetServer {
    sockets: HashMap<Token, TcpListener>,
    clients: Slab<TelnetClient>,
    //token: Option<Token>,
    token_counter: usize,
}

impl TelnetServer {
    pub fn new(socket:TcpListener) -> TelnetServer {
        let mut sockets = HashMap::new();
        sockets.insert(::SERVER_BIND_START, socket);

        TelnetServer {
            sockets: sockets,
            clients: Slab::with_capacity(1024),
            token_counter: ::SERVER_BIND_START.0,
        }
    }

    pub fn accept(&mut self, poll:&Poll, token:Token, history:Rc<RefCell<History>>){
        let (client_stream, client_addr) = match self.sockets[&token].accept() {
            Err(why) => {
                println!("Failed to accept connection: {}", why.description());
                return;
            },
            Ok((stream, addr)) => {
                ::push_info(&history, format!("[{}] Connection established\n", addr));
                self.poll_clients_write(&poll);
                (stream, addr)
            },
        };
        // Insert new client into client collection
        let interest = Ready::readable() | Ready::writable();
        let client = TelnetClient::new(client_stream, client_addr, interest, history);
        if let Ok(new_token) = self.clients.insert(client) {
            poll.register(self.clients[new_token].get_stream(), new_token, Ready::readable() | Ready::writable(),
                          PollOpt::edge() | PollOpt::oneshot()).unwrap();
        };
    }

    pub fn conn<'a>(&'a mut self, tok:Token) -> &'a mut TelnetClient {
        &mut self.clients[tok]
    }

    pub fn remove(&mut self, tok:Token) -> Option<TelnetClient> {
        self.clients.remove(tok)
    }

    pub fn socket(&mut self, tok:Token) -> Option<&TcpListener> {
        self.sockets.get(&tok)
    }

    pub fn poll_clients_write(&mut self, poll:& Poll){
        /*
        for &(tok, client) in &self.clients {
            poll.reregister(client.get_stream(), *tok, Ready::writable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }
        */
    }
}

