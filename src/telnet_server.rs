//use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
//use std::error::{Error};
//use mio::*;
//use mio::tcp::{TcpListener};
use tokio_core;
use tokio_core::net::{TcpListener};
use tokio_core::io::Io;
use futures::{self, Stream, Sink, Future};
use futures::sync::mpsc;
use std::io;
use std::io::Write;

//use telnet_client::TelnetClient;
use history::{History, HistoryReader};

use rust_telnet::codec::{TelnetCodec, TelnetIn};

use pty::PtySink;

use child;

//#[derive(PartialEq, Copy, Clone)]
//pub enum BindKind {
//    Control,
//    Log,
//}

//pub fn process(socket: TcpStream) -> Box<Future<Item=(), Error=()>> {
//    let fut = tokio_core::io::write_all(socket, b"hej!\r\n")
//        .map(|x| ())
//        .map_err(|e| ());
//    Box::new(fut)
//}

pub struct TelnetServer {
    process: Rc<RefCell<child::Process>>,
    history: Rc<RefCell<History>>,
    //child_writer: Rc<RefCell<PtySink>>,
    noinfo: bool,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
    tx: mpsc::Sender<(SocketAddr, TelnetIn)>,
    rx: mpsc::Receiver<(SocketAddr, TelnetIn)>,
}

impl TelnetServer {
    pub fn new(history: Rc<RefCell<History>>, process: Rc<RefCell<child::Process>>, noinfo: bool) -> TelnetServer {
        let (tx, rx) = mpsc::channel(2048);
        //let child_writer = process.borrow_mut().pty.input().take().unwrap();
        TelnetServer {
            process: process,
            history: history,
            //child_writer: Rc::new(RefCell::new(child_writer)),
            noinfo: noinfo,
            listeners: Vec::new(),
            tx: tx,
            rx: rx,
        }
    }

    pub fn bind(&mut self, addr: &SocketAddr, handle: tokio_core::reactor::Handle) {
        let listener = TcpListener::bind(addr, &handle).unwrap();
        let history = self.history.clone();
        let tx = self.tx.clone();
        let sserver = listener.incoming().and_then(move |(socket, peer_addr)| {
            let (writer, reader) = socket.framed(TelnetCodec::new()).split();

            let tx = tx.clone();
            let responses = tx.send_all(reader.map(move |x| (peer_addr, x)).map_err(|_| unimplemented!())).map_err(|_|());

            let messages = HistoryReader::new(history.clone());
            let server = writer
                .send_all(messages)
                .then(|_| Ok(()));

            let join = server.join(responses);
            handle.spawn(join.map(|_| ()));
            Ok(peer_addr)
        }).for_each(|peer_addr| {
            println!("lost connection {:?}", peer_addr);
            Ok(())
        });
        self.listeners.push(Box::new(sserver))
    }

    pub fn server(self) -> Box<Future<Item=(), Error=()>>{
        let process = self.process.clone();
        let child_writer = process.borrow_mut().pty.input().take().unwrap();
        //let child_writer = self.child_writer.take();
        let x = self.rx.for_each(move |(peer_addr, x)| {
            match x {
                TelnetIn::Text {text} => {
                    println!("hej {:?}", text);
                    return Ok(child_writer.send(text).map(|_| Ok(())))
                    //return Ok(child_writer.send(text).and_then(|x| {
                    //    x.flush();
                    //    Ok(())
                    //}));
                },
                TelnetIn::NAWS {rows, columns} => {
                    process.borrow_mut().set_window_size(peer_addr, (From::from(rows), From::from(columns)));
                },
                TelnetIn::Carriage => println!("CR"),
            }
            Ok(())
        });
        let server = futures::future::join_all(self.listeners).map(|_|()).map_err(|_|());
        return Box::new(x.join(server).map(|_|()));
    }

    //pub fn recv_process(&self) -> Box<Stream<Item=Vec<u8>, Error=io::Error>> {
    //    Box::new(self.child.output())
    //}

    //pub fn send_process(&self, msg) -> Box<Sink<SinkItem=Vec<u8>, SinkError=io::Error>> {
    //    Box::new(self.child.input())
    //}

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
