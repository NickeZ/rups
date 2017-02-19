use std::collections::{HashMap};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::net::{SocketAddr};
use std::error::{Error};
//use mio::*;
//use mio::tcp::{TcpListener};
use tokio_core;
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::io::Io;
use futures::{self, Stream, Sink, Poll, Async, Future};
use std::io;
use std::io::Write;
use std;

use telnet_client::TelnetClient;
use history::{History, HistoryReader};

use telnet::{TelnetCodec, TelnetIn};
use telnet::{IAC, OPTION};

use tty;

use child;

#[derive(PartialEq, Copy, Clone)]
pub enum BindKind {
    Control,
    Log,
}

pub fn process(socket: TcpStream) -> Box<Future<Item=(), Error=()>> {
    let fut = tokio_core::io::write_all(socket, b"hej!\r\n")
        .map(|x| ())
        .map_err(|e| ());
    Box::new(fut)
}

pub struct TelnetServer {
    process: Rc<RefCell<child::Process>>,
    history: Rc<RefCell<History>>,
    noinfo: bool,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
    min_window_size: (tty::Rows, tty::Columns),
}

impl TelnetServer {
    pub fn new(history: Rc<RefCell<History>>, process: Rc<RefCell<child::Process>>, noinfo: bool) -> TelnetServer {
        TelnetServer {
            process: process,
            history: history,
            noinfo: noinfo,
            listeners: Vec::new(),
            min_window_size: (From::from(u16::max_value()), From::from(u16::max_value())),
        }
    }

    pub fn bind(&mut self, addr: &SocketAddr, handle: tokio_core::reactor::Handle) {
        let listener = TcpListener::bind(addr, &handle).unwrap();
        let process_writer = Rc::new(RefCell::new(self.process.borrow().pty.register_input(&handle)));
        let history = self.history.clone();
        let process_clone = self.process.clone();
        //let history_reader = Rc::new(RefCell::new(self.history.borrow().reader()));
        let sserver = listener.incoming().for_each(move |(socket, peer_addr)| {
            let (writer, reader) = socket.framed(TelnetCodec::new()).split();

            let process_writer = process_writer.clone();
            let process_clone = process_clone.clone();
            //let history_clone = history.clone();

            let responses = reader.for_each(move |msg| {
                let mut pw_clone = process_writer.clone();
                let process_clone = process_clone.clone();
                //let history = history_clone.clone();
                //self.send_process(msg)
                match msg {
                    TelnetIn::Text {text} => {
                        //pw_clone.borrow_mut().send(text);
                        pw_clone.borrow_mut().ptyin.write(text.as_slice());
                        //println!("TEXT: {:?}", text);
                    },
                    TelnetIn::NAWS {rows, columns} => {
                        //self.process.pty.resize(rows, columns);
                        println!("resize to {:?} {:?}", rows, columns);
                        process_clone.borrow_mut().set_window_size(peer_addr, (rows, columns));
                    },
                    TelnetIn::Carriage => println!("CR"),
                }
                Ok(())
            }).map_err(|_| ());

            let messages = HistoryReader::new(history.clone());
            let server = writer
                .send_all(messages)
                .then(|_| Ok(()));
            //let server = writer.send("hej\r\n".as_bytes().to_vec()).then(|_| Ok(()));

            let join = server.join(responses);
            handle.spawn(join.map(|_| ()));
            Ok(())
        });
        self.listeners.push(Box::new(sserver))
    }

    pub fn server(mut self) -> Box<Future<Item=(), Error=io::Error>>{
        let server = futures::future::join_all(self.listeners).map(|x|());
        return Box::new(server);
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
