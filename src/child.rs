use std::process;
use std::cell::{RefCell};
use std::rc::{Rc};
//use std::io::prelude::*;
//use std::io::{self};
//use std::os::unix::io::{FromRawFd, AsRawFd};
use std::error::{Error};
use std::collections::HashMap;
use std::net::SocketAddr;

use pty;
use tokio_core::reactor::Handle;

//use pty::{TtyServer, FileDesc};
//use pty::ffi::{WinSize, set_winsize, get_winsize};
//use mio::deprecated::{PipeReader, PipeWriter};

use history::{History};

#[derive(Debug)]
pub enum ProcessError {
    ProcessAlreadySpawned,
}

#[allow(dead_code)]
pub struct Process {
    args: Vec<String>,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
    //ptyserver: TtyServer,
    child: Option<pty::Child>,
    //cid: Option<u32>,
    exit_status: Option<process::ExitStatus>,
    window_sizes: HashMap<SocketAddr, (pty::Rows, pty::Columns)>,
    //stdin: PipeWriter,
    //stdout: PipeReader,
    handle: Handle,
}

impl Process {
    pub fn new(args:Vec<String>, history:Rc<RefCell<History>>, foreground:bool, handle: Handle) -> Process {
        //pty.register(handle);
        Process {
            args: args,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
            //ptyserver: ptyserver,
            child: None,
            //cid: None,
            exit_status: None,
            window_sizes: HashMap::new(),
            //stdin: stdin,
            //stdout: stdout,
            handle: handle,
        }
    }

    pub fn spawn(&mut self) -> Result<(), ProcessError> {
        let pty = pty::Pty::new();
        //if self.cid.is_some() {
        //    return Err(ProcessError::ProcessAlreadySpawned);
        //}

        let mut command = process::Command::new(&self.args[0]);

        if self.args.len() > 1 {
            for arg in self.args[1..].iter() {
                command.arg(arg);
            }
        }

        match pty.spawn(command, &self.handle) {
            Err(why) => panic!("Couldn't spawn {}: {}", self.args[0], why.description()),
            Ok(child) => {
                self.child = Some(child);
                //self.cid = Some(p.id());
                //self.history.borrow_mut().push(
                //    HistoryType::Info,
                //    format!("Successfully spawned {}!\r\n", self.args[0]));
                println!("Launched {}", self.args[0]);
            }
        };
        Ok(())
    }

    pub fn wait(&mut self) {
        let child = self.child.take();
        if let Some(mut child) = child {
            child.wait();
        }
    }

    pub fn set_window_size(&mut self, addr: SocketAddr, ws: (pty::Rows, pty::Columns)) {
        println!("Store {:?},{:?} for {:?}", ws.0, ws.1, addr);
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

    //pub fn split(self) -> Result<(pty::PipeWriter, pty::PipeReader), ()> {
    //    match self.child {
    //        Some(child) => Ok((child.input(), child.output())),
    //        None => {
    //            println!("No child...");
    //            Err(())
    //        },
    //    }
    //}

    pub fn output(&mut self) -> Option<pty::PtyStream> {
        self.child.as_mut().unwrap().output().take()
    }

    pub fn input(&mut self) -> Option<pty::PtySink> {
        self.child.as_mut().unwrap().input().take()
    }

    //pub fn read(&mut self) {
    //    let mut buffer = [0;2048];
    //    let len = self.stdout.read(&mut buffer).unwrap();
    //    let line = match String::from_utf8(buffer[0..len].to_vec()) {
    //        Err(why) => {
    //            println!("Failed to parse utf8: {}", why);
    //            String::new()
    //        },
    //        Ok(line) => line,
    //    };
    //    //println!("[child stdout]: len {}, {}", line.len(), &line);
    //    if self.foreground {
    //        let _ = io::stdout().write_all(&buffer[0..len]);
    //        let _ = io::stdout().flush();
    //    }
    //    self.history.borrow_mut().push(HistoryType::Child, line);
    //}

    //pub fn stdin(&self) -> &PipeWriter {
    //    &self.stdin
    //}

    //pub fn stdout(&self) -> &PipeReader {
    //    &self.stdout
    //}

    //pub fn kill(&mut self) {
    //    if self.is_alive() {
    //        self.child.as_mut().unwrap().kill().expect("Failed to kill process");
    //        println!("TODO kill process..");
    //    }
    //}

    //pub fn is_alive(&self) -> bool {
    //    self.child.is_some() && self.exit_status.is_none()
    //}

    //pub fn send(&mut self, msg:String) {
    //    self.mailbox.push(msg);
    //}

    //pub fn write(&mut self) {
    //    for msg in self.mailbox.drain(..) {
    //        let _ = self.stdin.write(msg.as_bytes());
    //    }
    //}

    //pub fn wait(&mut self) -> &process::ExitStatus {
    //    if self.exit_status.is_some() {
    //        return self.exit_status.as_ref().unwrap();
    //    } else {
    //        self.exit_status = Some(self.child.as_mut().unwrap().wait().expect("Failed to wait on child"));
    //    }
    //    self.exit_status.as_ref().unwrap()
    //}
}
