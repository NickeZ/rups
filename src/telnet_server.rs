use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use std::error::{Error};
//use mio::*;
//use mio::tcp::{TcpListener};

use telnet_client::TelnetClient;
use history::History;

//type Slab<T> = ::slab::Slab<T, Token>;

#[derive(PartialEq, Copy, Clone)]
pub enum BindKind {
    Control,
    Log,
}

pub struct TelnetServer {
//    sockets: HashMap<Token, (TcpListener, BindKind)>,
//    clients: Slab<TelnetClient>,
//    token_counter: usize,
    noinfo: bool,
}

impl TelnetServer {
    pub fn new(noinfo: bool) -> TelnetServer {
        TelnetServer {
//            sockets: HashMap::new(),
//            clients: Slab::with_capacity(1024),
//            token_counter: ::SERVER_BIND_START.0,
            noinfo: noinfo,
        }
    }

//    pub fn add_bind(&mut self, poll: &Poll, addr:SocketAddr, kind:BindKind) {
//        if let Ok(listener) = TcpListener::bind(&addr) {
//            let tok = Token(self.token_counter);
//            poll.register(&listener, tok,
//                          Ready::readable(), PollOpt::edge()).unwrap();
//            self.sockets.insert(tok, (listener, kind));
//            self.token_counter += 1;
//
//        } else {
//            panic!("Failed to bind to port {}", addr);
//        }
//    }

    // Try to accept a connection, will return false if token is not a bind socket.
    //pub fn try_accept(&mut self, poll:&Poll, token:Token, history:Rc<RefCell<History>>) -> bool{
    //    if self.sockets.contains_key(&token) {
    //        let (ref socket, ref kind) = self.sockets[&token];
    //        let (client_stream, client_addr) = match socket.accept() {
    //            Err(why) => {
    //                println!("Failed to accept connection: {}", why.description());
    //                return false;
    //            },
    //            Ok((stream, addr)) => {
    //                ::push_info(&history, format!("[{}] Connection established\r\n", addr));
    //                self.poll_clients_write(&poll);
    //                (stream, addr)
    //            },
    //        };
    //        // Insert new client into client collection
    //        let client = TelnetClient::new(client_stream, client_addr, history, *kind, self.noinfo);
    //        if let Ok(new_token) = self.clients.insert(client) {
    //            let client = &mut self.clients[new_token];
    //            client.set_token(new_token);
    //            poll.register(client.get_stream(), new_token, Ready::writable(),
    //                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
    //        };
    //        return true;
    //    }
    //    false
    //}

    //pub fn conn<'a>(&'a mut self, tok:Token) -> &'a mut TelnetClient {
    //    &mut self.clients[tok]
    //}

    //pub fn remove(&mut self, tok:Token) -> Option<TelnetClient> {
    //    self.clients.remove(tok)
    //}

    /*
    pub fn socket(&mut self, tok:Token) -> Option<&TcpListener> {
        self.sockets.get(&tok)
    }
    */

    //pub fn poll_clients_write(&self, poll:& Poll){
    //    for client in self.clients.iter() {
    //        debug!("registering {:?} for writing", client.get_token().unwrap());
    //        poll.reregister(client.get_stream(), client.get_token().unwrap(), Ready::writable(),
    //                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
    //    }
    //}

//    pub fn get_window_size(&self) -> (u16, u16) {
//        let mut rows = u16::max_value();
//        let mut cols = u16::max_value();
//        for client in self.clients.iter() {
//            let (r, c) = client.window_size;
//            if r < rows {
//                rows = r;
//            }
//            if c < cols {
//                cols = c;
//            }
//        }
//        (rows, cols)
//    }
}

