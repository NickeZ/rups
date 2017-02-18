use std::{io, str};
use std::io::Cursor;

use tty;
use tokio_core::io::{Io, Codec, Framed, EasyBuf};
use rust_telnet::parser::{TelnetTokenizer, TelnetToken};
use byteorder::{BigEndian, ReadBytesExt};

#[allow(non_snake_case)]
pub mod IAC {
    pub const SE: u8 = 240;
    pub const NOP: u8 = 241;
    pub const DM: u8 = 242;
    pub const BRK: u8 = 243;
    pub const GA: u8 = 249;
    pub const SB: u8 = 250;
    pub const WILL: u8 = 251;
    pub const WONT: u8 = 252;
    pub const DO: u8 = 253;
    pub const DONT: u8 = 254;
    pub const IAC:u8 = 255;
}

#[allow(non_snake_case)]
pub mod OPTION {
    // Standard
    pub const EXOPL:u8 = 255;
    pub const TRANSMIT_BINARY:u8 = 0;
    pub const ECHO:u8 = 1;
    pub const SUPPRESS_GO_AHEAD:u8 = 3;
    pub const STATUS:u8 = 5;
    pub const TIMING_MARK:u8 = 6;
    // Draft
    pub const LINEMODE:u8 = 34;
    // Proposed
    pub const NAWS:u8 = 31;
}

enum TelnetCodecMode {
    Text,
    NAWS,
}

pub struct TelnetCodec {
    tokenizer: TelnetTokenizer,
    mode: TelnetCodecMode,
}

impl TelnetCodec {
    pub fn new() -> TelnetCodec {
        TelnetCodec {
            tokenizer: TelnetTokenizer::new(),
            mode: TelnetCodecMode::Text,
        }
    }
}

pub enum TelnetIn {
    Text {text:Vec<u8>},
    Carriage,
    NAWS {rows:tty::Rows, columns:tty::Columns},
}
//pub struct TelnetOut;

impl Codec for TelnetCodec {
    type In = TelnetIn;
    type Out = Vec<u8>;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<TelnetIn>> {
        let len = buf.len();
        if len == 0 {
            return Ok(None);
        }
        let mut res = Ok(None);
        let mut fin_len = 0;
        {
            let mut stream = self.tokenizer.tokenize(buf.as_slice());
            for token in stream.by_ref() {
                match token {
                    TelnetToken::Text(bytes) => {
                        match self.mode {
                            TelnetCodecMode::Text => {
                                //println!("text {:?} {}", bytes, str::from_utf8(bytes).unwrap_or(""));
                                res = Ok(Some(TelnetIn::Text{text: bytes.to_vec()}));
                                break;
                            },
                            TelnetCodecMode::NAWS => {
                                let mut rdr = Cursor::new(bytes);
                                let cols = From::from(rdr.read_u16::<BigEndian>().unwrap());
                                let rows = From::from(rdr.read_u16::<BigEndian>().unwrap());
                                res = Ok(Some(TelnetIn::NAWS{rows: rows, columns: cols}));
                                break;
                            }
                        }

                    },
                    TelnetToken::Command(command) => {
                        match command {
                            IAC::SE => {
                                match self.mode {
                                    TelnetCodecMode::NAWS => {
                                        self.mode = TelnetCodecMode::Text;
                                    },
                                    _ => (),
                                }
                            },
                            x => println!("unhandled command {:?}", command),
                        }
                    },
                    TelnetToken::Negotiation{command, channel} => {
                        match (command, channel) {
                            (IAC::SB, OPTION::NAWS) => {
                                self.mode = TelnetCodecMode::NAWS;
                            },
                            (_, _) => println!("unhandled negotiation {:?} {:?}", command, channel),
                        }
                    },
                }
            }
            fin_len = stream.data.len();
        }
        println!("Will drain {} from {}", len - fin_len, len);
        buf.drain_to(len - fin_len);
        res
    }

    fn encode(&mut self, msg: Vec<u8>, buf: &mut Vec<u8>) -> io::Result<()> {
        for c in msg {
            buf.push(c);
        }
        Ok(())
    }
}
