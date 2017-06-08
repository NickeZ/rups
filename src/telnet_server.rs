use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, Mutex};
use std::net::{SocketAddr};
use tokio_core::reactor;
use tokio_core::net::{TcpListener};
use tokio_io::AsyncRead;
use futures::{self, Stream, Sink, Future, BoxFuture};
use futures::stream::{BoxStream, once};
use futures::sync::mpsc;
use std::io;

use history::{History, HistoryReader};

use rust_telnet::codec::{TelnetCodec, TelnetIn};
use futures_addition::send_all;
use futures_addition::rx_wrapper::ReceiverWrapper;

use child;

//#[derive(PartialEq, Copy, Clone)]
//pub enum BindKind {
//    Control,
//    Log,
//}

pub struct TelnetServer {
    process: Arc<Mutex<child::Process>>,
    history: Rc<RefCell<History>>,
    noinfo: bool,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
    tx: mpsc::Sender<Vec<u8>>,
    rx: ReceiverWrapper<Vec<u8>>,
}

impl TelnetServer {
    pub fn new(history: Rc<RefCell<History>>, process: Arc<Mutex<child::Process>>, noinfo: bool) -> TelnetServer {
        // Create a channel for all telnet clients to put their data
        let (tx, rx) = mpsc::channel(2048);
        TelnetServer {
            process: process,
            history: history,
            noinfo: noinfo,
            listeners: Vec::new(),
            tx: tx,
            rx: ReceiverWrapper::new(rx),
        }
    }

    pub fn bind(&mut self, addr: &SocketAddr, handle: reactor::Handle, read_only: bool) {
        let listener = TcpListener::bind(addr, &handle).unwrap();
        println!("Listening on Port {}", addr);
        let history = self.history.clone();
        let tx = self.tx.clone();
        let process = self.process.clone();
        let sserver = listener.incoming().for_each(move |(socket, peer_addr)| {
            println!("Connection {:?}", peer_addr);
            let (writer, reader) = socket.framed(TelnetCodec::new()).split();
            let process = process.clone();

            // Send all outputs from the process to the telnet client
            let from_process = HistoryReader::new(history.clone());
            let server = writer
                .send_all(from_process)
                .then(|_| Ok(()));

            if read_only {
                handle.spawn(server);
                return Ok(());
            }
            // Create a new sender endpoint where this telnet client can
            // send all its outputs
            let tx = tx.clone();
            let reader = reader .filter_map(move |x| {
                let process = process.clone();
                match x {
                    TelnetIn::Text {text} => if text.len() == 1 {
                        // TODO: User customized commands
                        match text[0] {
                            0x12 => { // Ctrl-R
                                debug!("Receieved relaunch command");
                                let mut process = process.lock().unwrap();
                                let _ = process.spawn();
                                return None
                            },
                            0x14 => { // Ctrl-T
                                debug!("Receieved toggle autorestart command");
                                return None
                            },
                            0x18 => { // Ctrl-X
                                debug!("Received kill command");
                                let mut process = process.lock().unwrap();
                                process.kill().unwrap();
                                return None
                            },
                            _ => return Some(text),
                        }
                    },
                    // TODO: Wrong compiler warning?
                    TelnetIn::Text {text} => return Some(text),
                    TelnetIn::NAWS {rows, columns} => {
                        process.lock().unwrap().set_window_size(
                            peer_addr,
                            (From::from(rows), From::from(columns))
                        );
                    },
                    TelnetIn::Carriage => println!("CR"),
                }
                None
            }).map_err(|_| unimplemented!());
            let responses = tx.send_all(reader).map_err(|_| ());
            let server = server.join(responses).map(|_| ());
            handle.spawn(server);
            Ok(())
        });
        self.listeners.push(Box::new(sserver))
    }

    pub fn server(self, handle: reactor::Handle) -> Box<Future<Item=(), Error=()>>{
        let child_writers = child::ProcessWriters::new(self.process.clone());
        let process = self.process.clone();
        let rx = self.rx;
        let rx = rx.map_err(|_| io::Error::new(io::ErrorKind::Other, "mupp"));
        let x = child_writers.fold(rx, move |rx, writer| {
            send_all::new(writer, rx).then(|result| {
                let (_, mut rx, reason) = result.unwrap();
                match reason {
                    send_all::Reason::StreamEnded => Err(io::Error::new(io::ErrorKind::Other, "stream ended")),
                    send_all::Reason::SinkEnded{last_item} => {
                        rx.get_mut().undo();
                        Ok(rx)
                    },
                }
            })
        }).map_err(|_|()).map(|_|());
        let server = futures::future::join_all(self.listeners).map(|_|()).map_err(|_|());
        handle.spawn(server);
        Box::new(x)
    }
}
