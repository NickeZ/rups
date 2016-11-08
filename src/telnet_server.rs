use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use mio::*;
use mio::tcp::{TcpListener, TcpStream};

use telnet_client::TelnetClient;
use ::history::History;

pub struct TelnetServer {
    pub socket: TcpListener,
    pub clients: HashMap<Token, telnet_client::TelnetClient>,
    token_counter: usize,
}

impl TelnetServer {
    pub fn new(socket:TcpListener) -> TelnetServer {
        TelnetServer {
            socket: socket,
            clients: HashMap::new(),
            token_counter: ::TELNET_CLIENT_START.0,
        }
    }

    pub fn add_client(&mut self, stream:TcpStream, addr:SocketAddr,
                      history:Rc<RefCell<History>>) -> Token {
        let new_token = Token(self.token_counter);
        self.token_counter += 1;

        self.clients.insert(new_token,
                            TelnetClient::new(stream, addr,
                                              Ready::readable() | Ready::writable(), history));
        new_token
    }

    pub fn notify_clients(&mut self, poll:& Poll){
        for (tok, client) in &self.clients {
            poll.reregister(client.get_stream(), *tok, Ready::writable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }
    }
}

