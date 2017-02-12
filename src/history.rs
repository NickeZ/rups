use std::io;
use futures::{Stream, Sink, Poll, StartSend};

#[derive(Debug, PartialEq)]
pub enum HistoryType {
    Child,
    Info,
}

pub struct History {
    buffers: ::std::collections::VecDeque<(HistoryType, String)>,
    histsize: usize,
    offset: usize,
}

impl History {
    pub fn new(histsize:usize) -> History {
        History {
            buffers: ::std::collections::VecDeque::with_capacity(histsize),
            histsize: histsize,
            offset: 0,
        }
    }

    pub fn push(&mut self, kind:HistoryType, message:String) {
        if self.buffers.len() >= self.histsize {
            self.buffers.pop_front();
            self.offset += 1;
        }
        self.buffers.push_back((kind, message));
        debug!("buffers are now: {:?}", self.buffers);
    }

    pub fn get_from(&self, index:usize) -> ::std::iter::Skip<::std::collections::vec_deque::Iter<(HistoryType, String)>> {
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

    pub fn writer(&self) -> HistoryWriter {
        HistoryWriter::new(self)
    }

    pub fn reader(&self) -> HistoryReader {
        HistoryReader::new(self)
    }
}

struct HistoryWriter<'a> {
    history: &'a History,
}

impl<'a> HistoryWriter<'a> {
    pub fn new(history: &'a History) -> HistoryWriter {
        HistoryWriter {
            history: history,
        }
    }
}

impl<'a> Sink for HistoryWriter<'a> {
    type SinkItem = Vec<u8>;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        unimplemented!()
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        unimplemented!()
    }
}

struct HistoryReader<'a> {
    history: &'a History,
    index: u64,
}

impl<'a> HistoryReader<'a> {
    pub fn new(history: &'a History) -> HistoryReader {
        HistoryReader {
            history: history,
            index: 0,
        }
    }
}

impl<'a> Stream for HistoryReader<'a> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        unimplemented!()
    }

}
