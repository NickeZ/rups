use std::{io, str};

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
    fn new() -> TelnetCodec {
        TelnetCodec {
            tokenizer: TelnetTokenizer::new(),
            mode: TelnetCodecMode::Text,
        }
    }
}

pub enum TelnetIn {
    Text {text:str},
    Carriage,
    NAWS {rows:u16, cols:u16},
}
pub struct TelnetResponse;

impl Codec for TelnetCodec {
    type In = TelnetIn;
    type Out = TelnetResponse;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<TelnetIn>> {
        for token in self.tokenizer.tokenize(buf.as_slice()) {
            match token {
                TelnetToken::Text(bytes) => {
                    println!("text {:?} {}", bytes, str::from_utf8(bytes).unwrap_or(""));
                    Ok(Some(TelnetIn::Text{text: str::from_utf8(bytes).unwrap_or("")}))
                },
                TelnetToken::Command(command) => {
                    println!("command {:?}", command);
                },
                TelnetToken::Negotiation{command, channel} => {
                    println!("negotiation {:?}", command);
                },
            }
        }
        Ok(None)
    }

    fn encode(&mut self, msg: TelnetResponse, buf: &mut Vec<u8>) -> io::Result<()> {
        Ok(())
    }
}
