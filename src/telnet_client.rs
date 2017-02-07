use std::net::{SocketAddr};
use std::cell::RefCell;
use std::rc::Rc;
use std::io::prelude::*;
use std::io::Cursor;
use std::{str};
use mio::*;
use mio::tcp::{TcpStream};
use rust_telnet::parser::{TelnetTokenizer, TelnetToken};
//use rust_telnet::iac;
use time;
use byteorder::{BigEndian, ReadBytesExt};

use history::HistoryType;
use history::History;

use telnet::{IAC, OPTION};

use telnet_server::*;

const LINESEP:char = '\n';

#[derive(PartialEq)]
enum ClientState {
    Connected,
    HasSentMotd,
}

enum TelnetMode {
    Text,
    NAWS,
}

pub struct TelnetClient {
    //token: Option<Token>,
    addr: SocketAddr,
    stream: TcpStream,
    history: Rc<RefCell<History>>,
    cursor: usize,
    state: ClientState,
    tokenizer: TelnetTokenizer,
    server_echo: bool,
    pub window_size: (u16, u16),
    pub kind: BindKind,
    noinfo: bool,
    telnetmode: TelnetMode,
}

impl TelnetClient {
    pub fn new(stream:TcpStream, addr: SocketAddr,
               history:Rc<RefCell<History>>, kind:BindKind, noinfo:bool) -> TelnetClient {
        let cursor = history.borrow_mut().get_offset();
        TelnetClient {
            token: None,
            stream: stream,
            addr: addr,
            history: history,
            cursor:cursor,
            state: ClientState::Connected,
            tokenizer: TelnetTokenizer::new(),
            server_echo: true,
            window_size: (0, 0),
            kind: kind,
            noinfo: noinfo,
            telnetmode: TelnetMode::Text,
        }
    }

    //pub fn get_token(&self) -> Option<Token> {
    //    self.token
    //}
    //pub fn set_token(&mut self, token:Token) {
    //    self.token = Some(token);
    //}
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
            Err(why) => error!("Failed to read stream: {}", why),
            Ok(len) => {
                // Create a temporary String to concatenate the text tokens in
                let mut content = String::new();
                if len == 0 {
                    // If we read 0 bytes the other end probably hung up.
                    // Return an empty string.
                    return Some(content)
                }
                println!("new packet");
                for token in self.tokenizer.tokenize(&buffer[0..len]) {
                    println!("{:?}", token);
                    match token {
                        TelnetToken::Text(text) => {
                            debug!("token text: {:?}", text);
                            // Every time we receive a token that begins with a CR we send
                            // a newline instead to the process since the process runs on Linux.
                            match self.telnetmode {
                                TelnetMode::NAWS => {
                                    let mut rdr = Cursor::new(text);
                                    let cols = rdr.read_u16::<BigEndian>().unwrap();
                                    let rows = rdr.read_u16::<BigEndian>().unwrap();
                                    self.window_size = (rows, cols);
                                    println!("window size should be {} {}", rows, cols);
                                },
                                TelnetMode::Text => {
                                    if text[0] == '\r' as u8 {
                                        content.push(LINESEP);
                                    } else {
                                        match str::from_utf8(text) {
                                            Err(why) => error!("Failed to parse {:?}: {}", text, why),
                                            Ok(text) => {
                                                content.push_str(text);
                                            },
                                        }
                                    }
                                }
                            }
                        },
                        TelnetToken::Command(command) => {
                            match command {
                                IAC::SE => {
                                    match self.telnetmode {
                                        TelnetMode::NAWS => {
                                            self.telnetmode = TelnetMode::Text;
                                        },
                                        _ => (),
                                    }
                                },
                                _ => warn!("Unkown telnet Command {:?}", command),
                            }
                        },
                        TelnetToken::Negotiation{command, channel} => {
                            match (command, channel) {
                                (IAC::DO, OPTION::ECHO) => {
                                    self.server_echo = true
                                },
                                (IAC::DONT, OPTION::ECHO) => {
                                    self.server_echo = false
                                },
                                (IAC::DO, OPTION::SUPPRESS_GO_AHEAD) | (IAC::DONT, OPTION::SUPPRESS_GO_AHEAD) => {},
                                (IAC::WILL, OPTION::NAWS) => {},
                                (IAC::SB, OPTION::NAWS) => {
                                    self.telnetmode = TelnetMode::NAWS;
                                },
                                _ => warn!("Unknown negotiation command {:?} {}", command, channel),
                            }
                        }
                    }
                }
                return Some(content);
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
        let _ = self.stream.write(&[IAC::IAC, IAC::WILL, OPTION::ECHO]);
        let _ = self.stream.write(&[IAC::IAC, IAC::WILL, OPTION::SUPPRESS_GO_AHEAD]);
        let _ = self.stream.write(&[IAC::IAC, IAC::DO, OPTION::NAWS]);
    }

    pub fn write(&mut self) {
        if self.state == ClientState::Connected {
            self.write_motd();
            self.state = ClientState::HasSentMotd;
        }
        for line in self.history.borrow_mut().get_from(self.cursor) {
            if ! (line.0 == HistoryType::Info && self.noinfo) {
                if line.0 == HistoryType::Info {
                    let _ = self.stream.write(b"\x1B[33m");
                }
                let _ = self.stream.write(line.1.as_bytes());
                if line.0 == HistoryType::Info {
                    let _ = self.stream.write(b"\x1B[0m");
                }
            }
            self.cursor += 1;
        }
    }
}
