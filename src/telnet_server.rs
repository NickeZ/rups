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
    noinfo: bool,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
    tx: mpsc::Sender<(SocketAddr, TelnetIn)>,
    rx: mpsc::Receiver<(SocketAddr, TelnetIn)>,
}

impl TelnetServer {
    pub fn new(history: Rc<RefCell<History>>, process: Rc<RefCell<child::Process>>, noinfo: bool) -> TelnetServer {
        // Create a channel for all telnet clients to put their data
        let (tx, rx) = mpsc::channel(2048);
        TelnetServer {
            process: process,
            history: history,
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

            // Create a new sender endpoint to the channel
            let tx = tx.clone();
            let responses = tx.send_all(reader.map(move |x| (peer_addr, x))
                .map_err(|_| unimplemented!()))
                .map_err(|_|());

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
        let x = self.rx.filter_map(move |(peer_addr, x)| {
            match x {
                TelnetIn::Text {text} => {
                    println!("hej {:?}", text);
                    return Some(text)
                },
                TelnetIn::NAWS {rows, columns} => {
                    process.borrow_mut().set_window_size(peer_addr, (From::from(rows), From::from(columns)));
                },
                TelnetIn::Carriage => println!("CR"),
            }
            None
        }).map_err(|_| io::Error::new(io::ErrorKind::Other, "mupp"));
        let x = child_writer.send_all(x).map_err(|_|());
        let server = futures::future::join_all(self.listeners).map(|_|()).map_err(|_|());
        return Box::new(x.join(server).map(|_|()));
    }
}
