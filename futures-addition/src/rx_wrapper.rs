use futures::stream::Stream;
use futures::sync::mpsc;
use futures::{Async, Poll};
use std::clone::Clone;

pub struct ReceiverWrapper<T> {
    // Would prefer trait object..
    inner: mpsc::Receiver<T>,
    last_item: Option<T>,
}

impl<T> ReceiverWrapper<T> {
    pub fn new(inner: mpsc::Receiver<T>) -> ReceiverWrapper<T> {
        ReceiverWrapper {
            inner: inner,
            last_item: None,
        }
    }

    pub fn undo(&mut self, item: T) {
        self.last_item = Some(item);
    }
}

impl<T: Clone> Stream for ReceiverWrapper<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(item) = self.last_item.take() {
            return Ok(Async::Ready(Some(item)));
        }
        self.inner.poll()
    }
}
