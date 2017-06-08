#[test]
fn it_works() {
}

use std::{io, str};
use std::io::Cursor;

use tokio_io::codec;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BytesMut, BufMut};

use parser::{TelnetTokenizer, TelnetToken};

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
    decoder: Decoder,
}

impl TelnetCodec {
    pub fn new() -> TelnetCodec {
        TelnetCodec {
            decoder: Decoder::new(),
        }
    }
}

#[derive(Clone)]
pub enum TelnetIn {
    Text {text:Vec<u8>},
    Carriage,
    NAWS {rows:u16, columns:u16},
}
//pub struct TelnetOut;

impl codec::Decoder for TelnetCodec {
    type Item = TelnetIn;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let len = src.len();
        if len == 0 {
            return Ok(None);
        }
        let (res, remainder_len) = self.decoder.decode(src.as_ref());
        debug!("Will drain {} from {}", len - remainder_len, len);
        src.split_to(len - remainder_len);
        res
    }
}

impl codec::Encoder for TelnetCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.reserve(item.len());
        for c in item {
            dst.put(c);
        }
        Ok(())
    }
}

struct Decoder {
    tokenizer: TelnetTokenizer,
    mode: TelnetCodecMode,
}

impl Decoder {
    fn new() -> Decoder {
        Decoder {
            tokenizer: TelnetTokenizer::new(),
            mode: TelnetCodecMode::Text,
        }
    }

    fn decode(&mut self, buf: &[u8]) -> (io::Result<Option<TelnetIn>>, usize) {
        let mut res = Ok(None);
        let mut stream = self.tokenizer.tokenize(buf);
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
                        command => warn!("unhandled command {:?}", command),
                    }
                },
                TelnetToken::Negotiation{command, channel} => {
                    match (command, channel) {
                        (IAC::SB, OPTION::NAWS) => {
                            self.mode = TelnetCodecMode::NAWS;
                        },
                        (_, _) => warn!("unhandled negotiation {:?} {:?}", command, channel),
                    }
                },
            }
        }
        (res, stream.data.len())
    }
}
