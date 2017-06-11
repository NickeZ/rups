use std::io;
use std::io::Write;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::vec_deque::Iter;
use std::collections::VecDeque;
use std::iter::Skip;
use std::fs::{File, OpenOptions};

use futures::{Stream, Sink, Poll, StartSend, Async, AsyncSink};
use futures::task::{self, Task};

use options::Options;

#[derive(Debug, PartialEq)]
pub enum HistoryLine {
    Child {message: Vec<u8>},
    //Command (Vec<u8>),
    //Info {message: String},
}

pub struct History {
    buffers: VecDeque<HistoryLine>,
    histsize: usize,
    offset: usize,
    tasks: Vec<Task>,
    logfiles: Vec<File>,
}

impl History {
    pub fn new(options: &Options) -> History {
        let buf = VecDeque::with_capacity(options.history_size);
        let logfiles = options.logfiles.as_ref().map_or(Vec::new(), |logfiles| {
            logfiles.iter().map(|file| {
                OpenOptions::new().write(true).create(true).open(file).expect("Failed to open logfile")
            }).collect()
        });

        History {
            buffers: buf,
            histsize: options.history_size,
            offset: 0,
            tasks: Vec::new(),
            logfiles: logfiles,
        }
    }

    pub fn park(&mut self, task: Task) {
        self.tasks.push(task);
    }

    pub fn unpark(&mut self) {
        for task in self.tasks.drain(..) {
            task.notify();
        }
    }

    pub fn push(&mut self, line:HistoryLine) {
        // TODO make asynchronous
        let tmp:Vec<()> = self.logfiles.iter().map(|mut file| {
            match line {
                HistoryLine::Child{ref message} => {
                    file.write(message.as_slice()).unwrap();
                },
                //_ => (),
            }
        }).collect();
        if self.buffers.len() >= self.histsize {
            self.buffers.pop_front();
            self.offset += 1;
        }
        self.buffers.push_back(line);
        trace!("buffers are now: {:?}", self.buffers);
    }

    pub fn get_from(&self, index:usize) -> Skip<Iter<HistoryLine>> {
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
        let mut history = self.history.borrow_mut();
        history.push(HistoryLine::Child{message: item});
        history.unpark();
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
        let mut history = self.history.borrow_mut();
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
                //&HistoryLine::Command(ref cmd) => {
                //    let mut content = cmd.clone();
                //    res.append(&mut content);
                //},
                //e => println!("unkonwn entry {:?}", e),
            }
            self.index = self.index + 1;
        }
        if res.len() > 0 {
            Ok(Async::Ready(Some(res)))
        } else {
            let task = task::current();
            history.park(task);
            Ok(Async::NotReady)
        }
    }
}
