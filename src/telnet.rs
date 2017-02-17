use std::{io, str};

use tty;
use tokio_core::io::{Io, Codec, Framed, EasyBuf};
use rust_telnet::parser::{TelnetTokenizer, TelnetToken};

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
        let mut drain = 0;
        if let Some(token) = self.tokenizer.tokenize(buf.as_slice()).next() {
            match token {
                TelnetToken::Text(bytes) => {
                    println!("text {:?} {}", bytes, str::from_utf8(bytes).unwrap_or(""));
                    //let vec = Vec::new();
                    //for byte in bytes.iter() {
                    //    vec.push(byte);
                    //}
                    drain = bytes.len();
                    return Ok(Some(TelnetIn::Text{text: bytes.to_vec()}));
                },
                TelnetToken::Command(command) => {
                    println!("command {:?}", command);
                    drain = 1;
                    return Ok(None);
                },
                TelnetToken::Negotiation{command, channel} => {
                    println!("negotiation {:?} {:?}", command, channel);
                    drain = 2;
                    return Ok(None);
                },
            }
        }
        println!("Drain to {}", drain);
        buf.drain_to(drain);
        Ok(None)
    }

    fn encode(&mut self, msg: Vec<u8>, buf: &mut Vec<u8>) -> io::Result<()> {
        for c in msg {
            buf.push(c);
        }
        Ok(())
    }
}
