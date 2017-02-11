use std::process;
use std::cell::{RefCell};
use std::rc::{Rc};
use std::io::prelude::*;
use std::io::{self};
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::error::{Error};

use tty;

//use tty::{TtyServer, FileDesc};
//use tty::ffi::{WinSize, set_winsize, get_winsize};
//use mio::deprecated::{PipeReader, PipeWriter};

use history::{History, HistoryType};

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
    child: Option<tty::Child>,
    exit_status: Option<process::ExitStatus>,
    //stdin: PipeWriter,
    //stdout: PipeReader,
}

impl Process {
    pub fn new(args:Vec<String>, history:Rc<RefCell<History>>, foreground:bool) -> Process {
        Process {
            args: args,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
            //ttyserver: ttyserver,
            child: None,
            exit_status: None,
            //stdin: stdin,
            //stdout: stdout,
        }
    }

    pub fn spawn(&mut self) -> Result<(), ProcessError> {
        if self.child.is_some() {
            return Err(ProcessError::ProcessAlreadySpawned);
        }
        let mut command = tty::Command::new(&self.args[0]);
        if self.args.len() > 1 {
            for arg in self.args[1..].iter() {
                command.arg(arg);
            }
        }

        match command.spawn() {
            Err(why) => panic!("Couldn't spawn {}: {}", self.args[0], why.description()),
            Ok(p) => {
                self.child = Some(p);
                self.history.borrow_mut().push(
                    HistoryType::Info,
                    format!("Successfully spawned {}!\r\n", self.args[0]));
            }
        };
        Ok(())
    }

    pub fn output(&mut self) -> tty::PipeReader {
        self.child.unwrap().output()
    }

    pub fn input(&mut self) -> tty::PipeWriter {
        self.child.unwrap().input()
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

    pub fn kill(&mut self) {
        if self.is_alive() {
            //self.child.as_mut().unwrap().kill().expect("Failed to kill process");
            println!("TODO kill process..");
        }
    }

    pub fn is_alive(&self) -> bool {
        self.child.is_some() && self.exit_status.is_none()
    }

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
