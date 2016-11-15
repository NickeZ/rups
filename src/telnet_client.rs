use std::net::{SocketAddr};
use std::cell::RefCell;
use std::rc::Rc;
use std::io::prelude::*;
use std::{str};
use mio::*;
use mio::tcp::{TcpStream};
use telnet::parser::{TelnetTokenizer, TelnetToken};
use telnet::iac::*;
use time;

use history::HistoryType;
use history::History;

use telnet_server::*;

const LINESEP:char = '\n';

#[derive(PartialEq)]
enum ClientState {
    Connected,
    HasSentMotd,
}

pub struct TelnetClient {
    addr: SocketAddr,
    stream: TcpStream,
    pub interest: Ready,
    history: Rc<RefCell<History>>,
    cursor: usize,
    state: ClientState,
    tokenizer: TelnetTokenizer,
    server_echo: bool,
    pub kind: BindKind,
}

impl TelnetClient {
    pub fn new(stream:TcpStream, addr: SocketAddr,
               history:Rc<RefCell<History>>, kind:BindKind) -> TelnetClient {
        let cursor = history.borrow_mut().get_offset();
        TelnetClient {
            stream: stream,
            addr: addr,
            interest: Ready::writable(),
            history: history,
            cursor:cursor,
            state: ClientState::Connected,
            tokenizer: TelnetTokenizer::new(),
            server_echo: true,
            kind: kind,
        }
    }

    pub fn get_stream(&self) -> &TcpStream {
        &self.stream
    }

    pub fn get_addr(&self) -> &SocketAddr {
        &self.addr
    }

    pub fn read(&mut self) -> Option<String> {
        // Create a temporary buffer to read into
        let mut buffer = [0;2048];
        match self.stream.read(&mut buffer) {
            Err(why) => println!("Failed to read stream: {}", why),
            Ok(len) => {
                let mut content = None;
                for token in self.tokenizer.tokenize(&buffer[0..len]) {
                    match token {
                        TelnetToken::Text(text) => {
                            // Create a temporary String to convert the buffer from
                            content = Some(String::new());
                            debug!("token text: {:?}", text);
                            // Every time we receive a token that begins with a CR we send
                            // a newline instead to the process since the process runs on Linux.
                            if text[0] == '\r' as u8 {
                                content.as_mut().unwrap().push(LINESEP);
                            } else {
                                content.as_mut().unwrap().push_str(str::from_utf8(text).unwrap());
                            }
                        },
                        TelnetToken::Command(command) => {
                            debug!("Telnet Command {:?}", command);
                        },
                        TelnetToken::Negotiation{command, channel} => {
                            match (command, channel) {
                                (IAC::DO, 1) => {
                                    self.server_echo = true
                                },
                                (IAC::DONT, 1) => {
                                    self.server_echo = false
                                },
                                (IAC::DO, 3) | (IAC::DONT, 3) => {},
                                _ => debug!("Unsupported Negotiation {:?} {}", command, channel),
                            }
                        }
                    }
                }
                self.interest = Ready::readable();
                // If we receive a zero length string we interpret that as connection lost.
                if len == 0 {
                    self.interest = self.interest | Ready::hup();
                }
                return content;
            }
        }
        None
    }

    pub fn write_motd(&mut self) {
        let _ = self.stream.write(b"\x1B[33m");
        let _ = self.stream.write(b"Welcome to Simple Process Server 0.0.1\r\n");
        let _ = self.stream.write(b"Auto start is {}, Auto restart is {}\r\n");
        let _ = self.stream.write(b"^X to kill the child, ^T to toggle auto restart\r\n");
        let _ = self.stream.write(b"^R to (re)start the child\r\n");
        let now = time::strftime("%a, %d %b %Y %T %z", &time::now());
        let _ = self.stream.write(b"This server was started at: ");
        let _ = self.stream.write(now.unwrap().as_bytes());
        let _ = self.stream.write(b"\x1B[0m\r\n");
        let _ = self.stream.write(&[0xff, IAC::WILL, 1]);
        let _ = self.stream.write(&[0xff, IAC::WILL, 3]);
    }

    pub fn write(&mut self) {
        if self.state == ClientState::Connected {
            self.write_motd();
            self.state = ClientState::HasSentMotd;
        }
        for line in self.history.borrow_mut().get_from(self.cursor) {
            let preamble = match line.0 {
                HistoryType::Child => b"\x1B[39m",
                HistoryType::Info => b"\x1B[33m",
            };
            //let _ = self.stream.write(b"\r\n");
            let _ = self.stream.write(preamble);
            let _ = self.stream.write(line.1.replace("\n", "\r\n").as_bytes());
            let _ = self.stream.write(b"\x1B[0m");
            self.cursor += 1;
        }
        self.interest = Ready::readable();
    }
}
