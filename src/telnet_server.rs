use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, Mutex};
use std::net::{SocketAddr};
use tokio_core::reactor;
use tokio_core::net::{TcpListener};
use tokio_io::AsyncRead;
use futures::{self, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::stream;
use std::io;
use std::vec::IntoIter;
use std::path::PathBuf;

use history::{History, HistoryReader};

use rust_telnet::codec::{TelnetCodec, TelnetIn};
use rust_telnet::codec::{IAC, OPTION};
use futures_addition::send_all;
use futures_addition::rx_wrapper::ReceiverWrapper;

use child;
use options::Options;

pub struct TelnetServer {
    process: Arc<Mutex<child::Process>>,
    history: Rc<RefCell<History>>,
    options: Rc<RefCell<Options>>,
    listeners: Vec<Box<Future<Item=(), Error=io::Error>>>,
    tx: mpsc::Sender<Vec<u8>>,
    rx: ReceiverWrapper<Vec<u8>>,
}

impl TelnetServer {
    pub fn new(
        history: Rc<RefCell<History>>,
        process: Arc<Mutex<child::Process>>,
        options: Rc<RefCell<Options>>,
        ) -> TelnetServer
    {
        // Create a channel for all telnet clients to put their data
        let (tx, rx) = mpsc::channel(2048);
        TelnetServer {
            process: process,
            history: history,
            options: options,
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
        let options = self.options.clone();
        // Don't change commands at runtime
        let killcmd = self.options.borrow().killcmd;
        let togglecmd = self.options.borrow().togglecmd;
        let restartcmd = self.options.borrow().restartcmd;
        let autostart = self.options.borrow().autostart;
        let autorestart = self.options.borrow().autorestart;
        let sserver = listener.incoming().for_each(move |(socket, peer_addr)| {
            println!("Connection {:?}", peer_addr);
            let (writer, reader) = socket.framed(TelnetCodec::new()).split();
            let process = process.clone();
            let process2 = process.clone();
            let options = options.clone();
            let chdir = options.borrow().chdir.clone();
            let started_at = options.borrow().started_at.clone();

            // Send all outputs from the process to the telnet client
            let from_process = HistoryReader::new(history.clone());
            let server = writer
                .send_all(init_commands())
                .and_then(move |(rx, _tx)| rx.send_all(motd(killcmd, togglecmd, restartcmd, autostart, autorestart, chdir, started_at, process2)))
                .and_then(|(rx, _tx)| rx.send_all(from_process))
                .then(|_| Ok(()));

            // Return early if the client is bound to a read only port
            if read_only {
                handle.spawn(server);
                return Ok(());
            }

            // Filter out commands from telnet client
            let reader = reader.filter_map(move |x| {
                let process = process.clone();
                let options = options.clone();
                match x {
                    TelnetIn::Text {text} => {
                        if text.len() == 1 {
                            trace!("Received {:?}", text);
                            let cmd = text[0];
                            if cmd == restartcmd {
                                debug!("Receieved relaunch command");
                                let mut process = process.lock().unwrap();
                                let _ = process.spawn();
                                return None
                            }
                            if cmd == togglecmd {
                                options.borrow_mut().toggle_autorestart();
                                debug!("Receieved toggle autorestart command");
                                return None
                            }
                            if cmd == killcmd {
                                debug!("Received kill command");
                                let mut process = process.lock().unwrap();
                                process.kill().unwrap();
                                return None
                            }
                        }
                        return Some(text)
                    },
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

            // Create a new sender endpoint where this telnet client can
            // send all its outputs
            let tx = tx.clone();
            let responses = tx.send_all(reader).map_err(|_| ());
            let server = server.join(responses).map(|_| ());
            handle.spawn(server);
            Ok(())
        });
        self.listeners.push(Box::new(sserver))
    }

    pub fn server(self, handle: reactor::Handle) -> Box<Future<Item=(), Error=()>>{
        let child_writers = child::ProcessWriters::new(self.process.clone());
        let rx = self.rx.map_err(|_| io::Error::new(io::ErrorKind::Other, "mupp"));
        let x = child_writers.fold(rx, move |rx, writer| {
            send_all::new(writer, rx).then(|result| {
                let (_, mut rx, reason) = result.unwrap();
                match reason {
                    send_all::Reason::StreamEnded => Err(io::Error::new(io::ErrorKind::Other, "stream ended")),
                    send_all::Reason::SinkEnded{last_item} => {
                        if let Some(item) = last_item {
                            rx.get_mut().undo(item);
                        }
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

fn init_commands() -> stream::Iter<IntoIter<Result<Vec<u8>, io::Error>>> {
    stream::iter(vec![Ok(vec![IAC::IAC, IAC::WILL, OPTION::ECHO]),
                      Ok(vec![IAC::IAC, IAC::WILL, OPTION::SUPPRESS_GO_AHEAD]),
                      Ok(vec![IAC::IAC, IAC::DO,   OPTION::NAWS])])
}
pub fn motd(
    killcmd: u8,
    togglecmd: u8,
    restartcmd: u8,
    autostart: bool,
    autorestart: bool,
    chdir: PathBuf,
    started_at: String,
    process: Arc<Mutex<child::Process>>,
    ) -> stream::Iter<IntoIter<Result<Vec<u8>, io::Error>>>
{
    let child_started_at = if let Some(started_at) = process.lock().unwrap().started_at() {
        started_at.clone()
    } else {
        "Not started yet".to_owned()
    };
    stream::iter(
        vec![Ok(b"\x1B[33m".to_vec()),
             Ok(b"Welcome to Simple Process Server 0.0.1\r\n".to_vec()),
             Ok(format!("Auto start is {}, Auto restart is {}\r\n", autostart, autorestart).into_bytes()),
             Ok(format!("{} to kill the child, {} to toggle auto restart\r\n",
                        format_shortcut(killcmd),
                        format_shortcut(togglecmd)).into_bytes()),
             Ok(format!("{} to (re)start the child\r\n",
                        format_shortcut(restartcmd)).into_bytes()),
             Ok(format!("Child working dir: {}\r\n", chdir.display()).into_bytes()),
             Ok(b"The server was started at: ".to_vec()),
             Ok(started_at.as_bytes().to_vec()),
             Ok(b"\r\n".to_vec()),
             Ok(b"The child was started at: ".to_vec()),
             Ok(child_started_at.as_bytes().to_vec()),
             Ok(b"\r\n".to_vec()),
             Ok(b"\x1B[0m".to_vec())]
    )
}

pub fn format_shortcut(cmd: u8) -> String {
    match cmd {
        c if c < 32 => {
            let mut s = String::with_capacity(2);
            s.insert(0, '^');
            s.insert(1, (0x40 | c) as char);
            s
        },
        c => {
            let mut s = String::with_capacity(1);
            s.insert(0, c as char);
            s
        }
    }
}
