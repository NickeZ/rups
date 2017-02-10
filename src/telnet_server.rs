use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use std::error::{Error};
//use mio::*;
//use mio::tcp::{TcpListener};
use tokio_core;
use tokio_core::net::{TcpListener, TcpStream};
use futures::{self, Stream, Poll, Async, Future};
use std::io;
use std;

use telnet_client::TelnetClient;
use history::History;

#[derive(PartialEq, Copy, Clone)]
pub enum BindKind {
    Control,
    Log,
}

pub fn process(socket: TcpStream) -> Box<Future<Item=(), Error=()>> {
    let fut = tokio_core::io::write_all(socket, b"hej!\r\n")
        .map(|(a, buf)| {()})
        .map_err(|e| {()});
    Box::new(fut)
}

pub struct TelnetServer {
    noinfo: bool,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
}

impl TelnetServer {
    pub fn new(noinfo: bool) -> TelnetServer {
        TelnetServer {
            noinfo: noinfo,
            listeners: Vec::new(),
        }
    }

    pub fn bind(&mut self, addr: &SocketAddr, handle: &tokio_core::reactor::Handle) {
        let listener = TcpListener::bind(addr, handle).unwrap();
        let server = listener.incoming().and_then(|(socket, peer_addr)| {
            println!("connection {:?}", peer_addr);
            tokio_core::io::write_all(socket, b"hej!\r\n")
        }).for_each(|(_socket, _peer_addr)| {
            Ok(())
        });
        self.listeners.push(Box::new(server))
    }

    pub fn server(mut self, handle: &tokio_core::reactor::Handle) -> Box<Future<Item=(), Error=io::Error>>{
        let server = futures::future::join_all(self.listeners).map(|x|());
        return Box::new(server);
    }


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
