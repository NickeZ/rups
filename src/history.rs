use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

use futures::{Stream, Sink, Poll, StartSend, Async, AsyncSink};

use telnet::{IAC, OPTION};

#[derive(Debug, PartialEq)]
pub enum HistoryLine {
    Child {message: Vec<u8>},
    Command (Vec<u8>),
    Info {message: String},
}

pub struct History {
    buffers: VecDeque<HistoryLine>,
    histsize: usize,
    offset: usize,
}

impl History {
    pub fn new(histsize:usize) -> History {
        let mut buf = VecDeque::with_capacity(histsize);
        // TODO(nc): This works until the buffer has been filled...
        buf.push_back(HistoryLine::Command(vec![IAC::IAC, IAC::WILL, OPTION::ECHO]));
        buf.push_back(HistoryLine::Command(vec![IAC::IAC, IAC::WILL, OPTION::SUPPRESS_GO_AHEAD]));
        buf.push_back(HistoryLine::Command(vec![IAC::IAC, IAC::DO, OPTION::NAWS]));
        History {
            buffers: buf,
            histsize: histsize,
            offset: 0,
        }
    }

    pub fn push(&mut self, line:HistoryLine) {
        if self.buffers.len() >= self.histsize {
            self.buffers.pop_front();
            self.offset += 1;
        }
        self.buffers.push_back(line);
        debug!("buffers are now: {:?}", self.buffers);
    }

    pub fn get_from(&self, index:usize) -> ::std::iter::Skip<::std::collections::vec_deque::Iter<HistoryLine>> {
        let idx = if index < self.offset {
            0
        } else {
            index - self.offset
        };
        self.buffers.iter().skip(idx)
    }

    // offset is the index a reader should start from
    pub fn get_offset(&self) -> usize {
        self.offset
    }

    //pub fn writer(&self) -> HistoryWriter {
    //    HistoryWriter::new(self)
    //}

    //pub fn reader(&self) -> HistoryReader {
    //    HistoryReader::new(self)
    //}
}

pub struct HistoryWriter {
    history: Rc<RefCell<History>>,
}

impl HistoryWriter {
    pub fn new(history: Rc<RefCell<History>>) -> HistoryWriter {
        HistoryWriter {
            history: history,
        }
    }
}

impl Sink for HistoryWriter {
    type SinkItem = Vec<u8>;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.history.borrow_mut().push(HistoryLine::Child{message: item});
        Ok(AsyncSink::Ready)
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}

pub struct HistoryReader {
    history: Rc<RefCell<History>>,
    index: usize,
    first: bool,
}

impl HistoryReader {
    pub fn new(history: Rc<RefCell<History>>) -> HistoryReader {
        HistoryReader {
            history: history,
            index: 0,
            first: true,
        }
    }
}

impl Stream for HistoryReader {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let history = self.history.borrow();
        if self.first {
            self.first = false;
            self.index = history.get_offset();
        }
        let mut res = Vec::new();
        for entry in history.get_from(self.index) {
            match entry {
                &HistoryLine::Child{ref message} => {
                    let mut content = message.clone();
                    res.append(&mut content);
                },
                &HistoryLine::Command(ref cmd) => {
                    let mut content = cmd.clone();
                    res.append(&mut content);
                },
                e => println!("unkonwn entry {:?}", e),
            }
            self.index = self.index + 1;
        }
        if res.len() > 0 {
            Ok(Async::Ready(Some(res)))
        } else {
            ::futures::task::park().unpark();
            Ok(Async::NotReady)
        }
    }

}
