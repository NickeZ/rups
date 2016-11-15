#[derive(Debug)]
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
}
