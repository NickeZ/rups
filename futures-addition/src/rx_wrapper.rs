use futures::sync::mpsc;
use futures::stream::Stream;
use futures::{Poll, Async};
use std::clone::Clone;

pub struct ReceiverWrapper<T> {
    // Would prefer trait object..
    inner: mpsc::Receiver<T>,
    last_item: Option<T>,
    give_last: bool,
}

impl<T> ReceiverWrapper<T> {
    pub fn new(inner: mpsc::Receiver<T>) -> ReceiverWrapper<T> {
        ReceiverWrapper {
            inner: inner,
            last_item: None,
            give_last: false,
        }
    }

    pub fn undo(&mut self) {
        self.give_last = true;
    }
}

impl<T: Clone> Stream for ReceiverWrapper<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.give_last {
            self.give_last = false;
            if let Some(item) = self.last_item.take() {
                return Ok(Async::Ready(Some(item)))
            }
        }
        match try!(self.inner.poll()) {
            Async::Ready(t) => {
                self.last_item = t.clone();
                return Ok(Async::Ready(t));
            },
            Async::NotReady =>  Ok(Async::NotReady),
        }
    }
}
