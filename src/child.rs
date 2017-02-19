use std::process;
use std::cell::{RefCell};
use std::rc::{Rc};
use std::io::prelude::*;
use std::io::{self};
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::error::{Error};
use std::collections::HashMap;
use std::net::SocketAddr;

use tty;
use tokio_core::reactor::Handle;

//use tty::{TtyServer, FileDesc};
//use tty::ffi::{WinSize, set_winsize, get_winsize};
//use mio::deprecated::{PipeReader, PipeWriter};

use history::{History};

enum ProcessError {
    ProcessAlreadySpawned,
}

#[allow(dead_code)]
pub struct Process {
    args: Vec<String>,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
    //ttyserver: TtyServer,
    pub pty: tty::Pty,
    cid: Option<u32>,
    exit_status: Option<process::ExitStatus>,
    window_sizes: HashMap<SocketAddr, (tty::Rows, tty::Columns)>
    //stdin: PipeWriter,
    //stdout: PipeReader,
}

impl Process {
    pub fn new(args:Vec<String>, history:Rc<RefCell<History>>, foreground:bool, handle: &Handle) -> Process {
        let mut pty = tty::Pty::new(&args[0], handle);
        //pty.register(handle);
        if args.len() > 1 {
            for arg in args[1..].iter() {
                pty.arg(arg);
            }
        }
        Process {
            args: args,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
            //ttyserver: ttyserver,
            pty: pty,
            cid: None,
            exit_status: None,
            window_sizes: HashMap::new(),
            //stdin: stdin,
            //stdout: stdout,
        }
    }

    pub fn spawn(&mut self) -> Result<(), ProcessError> {
        //if self.child.is_some() {
        //    return Err(ProcessError::ProcessAlreadySpawned);
        //}
        //let mut command = tty::Command::new(&self.args[0]);
        //if self.args.len() > 1 {
        //    for arg in self.args[1..].iter() {
        //        command.arg(arg);
        //    }
        //}

        match self.pty.spawn() {
            Err(why) => panic!("Couldn't spawn {}: {}", self.args[0], why.description()),
            Ok(p) => {
                self.cid = Some(p.id());
                //self.history.borrow_mut().push(
                //    HistoryType::Info,
                //    format!("Successfully spawned {}!\r\n", self.args[0]));
                println!("Launched {}", self.args[0]);
            }
        };
        Ok(())
    }

    pub fn set_window_size(&mut self, addr: SocketAddr, ws: (tty::Rows, tty::Columns)) {
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
        self.pty.set_window_size(min_ws.0, min_ws.1);
    }

    //pub fn split(self) -> Result<(tty::PipeWriter, tty::PipeReader), ()> {
    //    match self.child {
    //        Some(child) => Ok((child.input(), child.output())),
    //        None => {
    //            println!("No child...");
    //            Err(())
    //        },
    //    }
    //}

    //pub fn output(&mut self) -> tty::PipeReader {
    //    self.child.unwrap().output()
    //}

    //pub fn input(&mut self) -> tty::PipeWriter {
    //    self.child.unwrap().input()
    //}

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

    pub fn kill(&mut self) {
        //if self.is_alive() {
        //    //self.child.as_mut().unwrap().kill().expect("Failed to kill process");
        //    println!("TODO kill process..");
        //}
    }

    //pub fn is_alive(&self) -> bool {
    //    self.child.is_some() && self.exit_status.is_none()
    //}

    pub fn send(&mut self, msg:String) {
        self.mailbox.push(msg);
    }

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
