pub enum HistoryType {
    Child,
    Info,
}

pub struct History {
    buffers: Vec<(HistoryType, String)>,
}

impl History {
    pub fn new() -> History {
        History {
            buffers: Vec::new(),
        }
    }

    pub fn push(&mut self, kind:HistoryType, message:String) {
        self.buffers.push((kind, message));
    }

    pub fn get_from(&self, index:usize) -> &[(HistoryType, String)] {
        &self.buffers[index..]
    }
}
