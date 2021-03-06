use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use futures::task::{self, Task};
use futures::{Async, Poll, Stream};

use pty;
use time;
use tokio_core::reactor::Handle;

use history::History;

#[derive(Debug)]
pub enum ProcessError {
    ProcessAlreadySpawned,
    NoChild,
    IoError(io::Error),
}

impl From<io::Error> for ProcessError {
    fn from(err: io::Error) -> Self {
        ProcessError::IoError(err)
    }
}

#[allow(dead_code)]
pub struct Process {
    args: Vec<String>,
    chdir: PathBuf,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
    child: Option<pty::Child>,
    exit_status: Option<process::ExitStatus>,
    window_sizes: HashMap<SocketAddr, (pty::Rows, pty::Columns)>,
    stdin: Option<pty::PtySink>,
    stdout: Option<pty::PtyStream>,
    handle: Handle,
    pr_task: Option<Task>,
    pw_task: Option<Task>,
    started_at: Option<String>,
}

impl Process {
    pub fn new(
        args: Vec<String>,
        chdir: PathBuf,
        history: Rc<RefCell<History>>,
        foreground: bool,
        handle: Handle,
    ) -> Process {
        Process {
            args: args,
            chdir: chdir,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
            child: None,
            exit_status: None,
            window_sizes: HashMap::new(),
            stdin: None,
            stdout: None,
            handle: handle,
            pr_task: None,
            pw_task: None,
            started_at: None,
        }
    }

    pub fn spawn(&mut self) -> Result<(), ProcessError> {
        if self.child.is_some() {
            return Err(ProcessError::ProcessAlreadySpawned);
        }
        let pty = pty::Pty::new();

        let mut command = process::Command::new(&self.args[0]);

        if self.args.len() > 1 {
            for arg in self.args[1..].iter() {
                command.arg(arg);
            }
        }
        command.current_dir(&self.chdir);

        match pty.spawn(command, &self.handle) {
            Err(why) => panic!("Couldn't spawn {}: {}", self.args[0], why.description()),
            Ok(child) => {
                self.child = Some(child);
                self.stdin = Some(self.child.as_mut().unwrap().input().take().unwrap());
                self.stdout = Some(self.child.as_mut().unwrap().output().take().unwrap());
                if let Some(task) = self.pr_task.take() {
                    task.notify();
                }
                if let Some(task) = self.pw_task.take() {
                    task.notify();
                }
                self.started_at = Some(
                    time::strftime("%a, %d %b %Y %T %z", &time::now())
                        .expect("Failed to format time"),
                );
                println!("Launched {}", self.args[0]);
            }
        };
        Ok(())
    }

    pub fn started_at(&self) -> Option<&String> {
        self.started_at.as_ref()
    }

    pub fn wait(&mut self) -> Result<process::ExitStatus, ProcessError> {
        if let Some(mut child) = self.child.take() {
            return child.wait().map_err(|e| From::from(e));
        }
        Err(ProcessError::NoChild)
    }

    pub fn kill(&mut self) -> Result<(), ProcessError> {
        if let Some(ref mut child) = self.child {
            return child.kill().map_err(|e| From::from(e));
        }
        Err(ProcessError::NoChild)
    }

    pub fn id(&self) -> Option<u32> {
        if let Some(ref child) = self.child {
            Some(child.id())
        } else {
            None
        }
    }

    pub fn set_window_size(&mut self, addr: SocketAddr, ws: (pty::Rows, pty::Columns)) {
        //println!("Store {:?},{:?} for {:?}", ws.0, ws.1, addr);
        self.window_sizes.insert(addr, ws);
        let mut min_ws = (From::from(u16::max_value()), From::from(u16::max_value()));
        for (_, ws) in &self.window_sizes {
            if ws.0 < min_ws.0 {
                min_ws.0 = ws.0;
            }
            if ws.1 < min_ws.1 {
                min_ws.1 = ws.1;
            }
        }
        if let Some(ref mut child) = self.child {
            child.set_window_size(min_ws.0, min_ws.1);
        }
    }

    pub fn set_pr_task(&mut self, task: Task) {
        self.pr_task = Some(task);
    }

    pub fn set_pw_task(&mut self, task: Task) {
        self.pw_task = Some(task);
    }
}

pub struct ProcessWriters {
    inner: Arc<Mutex<Process>>,
}

impl ProcessWriters {
    pub fn new(inner: Arc<Mutex<Process>>) -> ProcessWriters {
        ProcessWriters { inner: inner }
    }
}

impl Stream for ProcessWriters {
    type Item = pty::PtySink;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(stdin) = self.inner.lock().unwrap().stdin.take() {
            return Ok(Async::Ready(Some(stdin)));
        }
        self.inner.lock().unwrap().set_pw_task(task::current());
        Ok(Async::NotReady)
    }
}

pub struct ProcessReaders {
    inner: Arc<Mutex<Process>>,
}

impl ProcessReaders {
    pub fn new(inner: Arc<Mutex<Process>>) -> ProcessReaders {
        ProcessReaders { inner: inner }
    }
}

impl Stream for ProcessReaders {
    type Item = pty::PtyStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(stdout) = self.inner.lock().unwrap().stdout.take() {
            return Ok(Async::Ready(Some(stdout)));
        }
        self.inner.lock().unwrap().set_pr_task(task::current());
        Ok(Async::NotReady)
    }
}
