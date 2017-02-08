use std::process::{self, Command, ExitStatus};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::io::prelude::*;
use std::io::{self};
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::error::{Error};

//use tty::{TtyServer, FileDesc};
//use tty::ffi::{WinSize, set_winsize, get_winsize};
//use mio::deprecated::{PipeReader, PipeWriter};

use history::{History, HistoryType};

#[allow(dead_code)]
pub struct Child {
    args: Vec<String>,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
    //ttyserver: TtyServer,
    //child: Option<tty::Child>,
    exit_status: Option<process::ExitStatus>,
    //stdin: PipeWriter,
    //stdout: PipeReader,
}

impl Child {
    pub fn new(args:Vec<String>, history:Rc<RefCell<History>>, foreground:bool, spawn:bool) -> Child {
        let mut command = Command::new(&args[0]);
        if args.len() > 1 {
            command.args(&args[1..]);
        }
        let mut ttyserver = match TtyServer::new(None as Option<&FileDesc>) {
            Err(why) => panic!("Error, could not open tty: {}", why),
            Ok(s) => s,
        };
        // TODO: figure out if there are any benefits to inherit from stdin..
        //let stdin = FileDesc::new(libc::STDOUT_FILENO, false);
        //let mut ttyserver = match TtyServer::new(Some(&stdin)) {
        let mut child = None;
        if spawn {
            match ttyserver.spawn(command) {
                Err(why) => panic!("Couldn't spawn {}: {}", args[0], why.description()),
                Ok(p) => {
                    child = Some(p);
                    history.borrow_mut().push(
                        HistoryType::Info,
                        format!("Successfully launched {}!\r\n", args[0]));
                }
            };
        };

        let fd = FileDesc::new(ttyserver.get_master().as_raw_fd(), false);
        let fd2 = fd.dup().unwrap();
        let stdin = unsafe { PipeWriter::from_raw_fd(fd.as_raw_fd())};
        let stdout = unsafe { PipeReader::from_raw_fd(fd2.as_raw_fd())};
        Child {
            args: args,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
            ttyserver: ttyserver,
            child: child,
            exit_status: None,
            stdin: stdin,
            stdout: stdout,
        }
    }

    pub fn new_from_child(other: Child) -> Child {
        Child::new(other.args, other.history, other.foreground, true)
    }

    pub fn read(&mut self) {
        let mut buffer = [0;2048];
        let len = self.stdout.read(&mut buffer).unwrap();
        let line = match String::from_utf8(buffer[0..len].to_vec()) {
            Err(why) => {
                println!("Failed to parse utf8: {}", why);
                String::new()
            },
            Ok(line) => line,
        };
        //println!("[child stdout]: len {}, {}", line.len(), &line);
        if self.foreground {
            let _ = io::stdout().write_all(&buffer[0..len]);
            let _ = io::stdout().flush();
        }
        self.history.borrow_mut().push(HistoryType::Child, line);
    }

    pub fn stdin(&self) -> &PipeWriter {
        &self.stdin
    }

    pub fn stdout(&self) -> &PipeReader {
        &self.stdout
    }

    pub fn kill(&mut self) {
        if self.is_alive() {
            self.child.as_mut().unwrap().kill().expect("Failed to kill process");
        }
    }

    pub fn is_alive(&self) -> bool {
        self.child.is_some() && self.exit_status.is_none()
    }

    pub fn send(&mut self, msg:String) {
        self.mailbox.push(msg);
    }

    pub fn write(&mut self) {
        for msg in self.mailbox.drain(..) {
            let _ = self.stdin.write(msg.as_bytes());
        }
    }

    pub fn wait(&mut self) -> &ExitStatus {
        if self.exit_status.is_some() {
            return self.exit_status.as_ref().unwrap();
        } else {
            self.exit_status = Some(self.child.as_mut().unwrap().wait().expect("Failed to wait on child"));
        }
        self.exit_status.as_ref().unwrap()
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let mut ws = get_winsize(&self.stdin).unwrap();
        ws.ws_row = rows;
        ws.ws_col = cols;
        set_winsize(&self.stdin, &ws);
    }
}

