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

//type Slab<T> = ::slab::Slab<T, Token>;

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
//    sockets: HashMap<Token, (TcpListener, BindKind)>,
//    clients: Slab<TelnetClient>,
//    token_counter: usize,
    noinfo: bool,
    listeners: Vec<TcpListener>,
    //first: bool,
}

impl TelnetServer {
    pub fn new(noinfo: bool) -> TelnetServer {
        TelnetServer {
//            sockets: HashMap::new(),
//            clients: Slab::with_capacity(1024),
//            token_counter: ::SERVER_BIND_START.0,
            noinfo: noinfo,
            listeners: Vec::new(),
            //first: false,
        }
    }

    pub fn bind(&mut self, addr: &SocketAddr, handle: &tokio_core::reactor::Handle) {
        let listener = TcpListener::bind(addr, handle).unwrap();
        self.listeners.push(listener);
        //if !self.first {
        //    self.listener.push(listener);
        //    self.first = true;
        //    return;
        //}

        //handle.spawn(incoming);
    }

    //pub fn server(&mut self) -> futures::stream::ForEach<TcpStream, (), ()> {
    pub fn server(&mut self, handle: &tokio_core::reactor::Handle, handle2: &tokio_core::reactor::Handle) -> Box<Future<Item=(), Error=io::Error>>{
        if self.listeners.len() > 1 {
            for listener in self.listeners.drain(1..) {
                let server = listener.incoming().for_each(|(socket, peer_addr)| {
                    println!("connection {:?}", peer_addr);
                    handle.spawn(process(socket));
                    //tokio_core::io::write_all(socket, b"hej!\r\n");
                    Ok(())
                }).map_err(|e| {()});
                handle2.spawn(server);
            }
        }
        Box::new(self.listeners.remove(0).incoming().for_each(|(socket, peer_addr)| {
            println!("connection on 0");
            handle2.spawn(process(socket));
            Ok(())
        }))
    }

    //pub fn incoming(mut self) -> tokio_core::net::Incoming {
    //    let listener = self.listener.remove(0);
    //    listener.incoming()
    //}
    //pub fn incoming(self) -> Box<Future> {
    //    let mut futures = self.connections.remove(0);
    //    if self.connections.len() > 0 {
    //        for listener in self.listeners.drain(0..) {
    //            futures.select(listener);
    //        }
    //    }
    //    Box::new(futures)
    //    //Incoming::new(self.listeners)
    //}

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

//fn combinator<S>(stream_list: Vec<S>) -> Box<Stream<Item=(TcpStream, SocketAddr), Error=io::Error>>
//    where S: Stream<Item=(TcpStream, SocketAddr), Error=io::Error>
//{
//    if stream_list.len() == 2 {
//        return Box::new(stream_list.remove(0).select(stream_list.remove(0)));
//    }
//    let stream1 = stream_list.remove(0);
//    let stream2 = stream_list.remove(0);
//    stream_list.push(stream1.select(stream2));
//    combinator(stream_list)
//}
//
//struct Incoming {
//    inner: Box<futures::Stream<Item=(TcpStream, SocketAddr), Error=io::Error>>,
//}
//
//impl Incoming {
//    pub fn new(mut listeners: Vec<TcpListener>) -> Incoming {
//        if listeners.len() == 0 {
//            panic!("No binds");
//        }
//        let incomings = Vec::new();
//        for listener in listeners.drain(0..) {
//            incomings.push(listener.incoming());
//        }
//        if incomings.len() > 1 {
//            let incoming = combinator(incomings);
//            return Incoming {inner: Box::new(incoming)};
//        }
//        //for item in incomings.drain(0..) {
//        //    incoming = incoming.select(item);
//        //}
//        //let incoming = listeners.remove(0).incoming();
//        //if listeners.len() > 0 {
//        //    loop {
//        //        let incoming = listeners.remove(0).incoming().select(incoming);
//        //        if listeners.len() == 0 {
//        //            return Incoming {
//        //                inner: Box::new(incoming),
//        //            };
//        //        }
//        //    }
//        //}
//        //let mut select;
//        ////let mut tmp;
//        //let mut index = 0;
//        //if listeners.len() > 0 {
//        //    for l in listeners.drain(0..) {
//        //        if(index == 0) {
//        //            select = incoming.select(l.incoming());
//        //        } else {
//        //            select = select.select(l.incoming());
//        //        }
//        //        //tmp = &select;
//        //        index = index + 1;
//        //    }
//        //    //let incoming_merge = incoming.merge(listeners.remove(0).incoming());
//        //    //loop {
//        //    //    if listeners.len() == 0 {
//        //    //        break;
//        //    //    }
//        //    //    incoming_merge = incoming_merge.merge(listeners.remove(0).incoming());
//        //    //}
//        //    //return Incoming {
//        //    //    inner: Box::new(incoming),
//        //    //}
//        //}
//        Incoming {
//            inner: Box::new(incomings.remove(0)),
//        }
//    }
//}
//
//impl Stream for Incoming {
//    type Item = (TcpStream, SocketAddr);
//    type Error = io::Error;
//
//    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
//        //futures::task::park().unpark();
//        self.inner.poll()
//        //Ok(Async::NotReady)
//    }
//}

