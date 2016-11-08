use std::process::{self, Command, ExitStatus};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::io::prelude::*;
use std::io::{self};
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::error::{Error};

use tty::{TtyServer, FileDesc};
use mio::deprecated::{PipeReader, PipeWriter};

use history::{History, HistoryType};

#[allow(dead_code)]
pub struct Child {
    child: process::Child,
    ttyserver: TtyServer,
    pub stdin: PipeWriter,
    pub stdout: PipeReader,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
}

impl Child {
    pub fn new(commands:&Vec<String>, history:Rc<RefCell<History>>, foreground:bool) -> Child {
        let (executable, args) = commands.split_first().unwrap();
        let mut command = Command::new(executable);
        if args.len() > 0 {
            command.args(&args);
        }
        // TODO: figure out if there are any benefits to inherit from stdin..
        //let stdin = FileDesc::new(libc::STDOUT_FILENO, false);
        //let mut ttyserver = match TtyServer::new(Some(&stdin)) {
        let mut ttyserver = match TtyServer::new(None as Option<&FileDesc>) {
            Ok(s) => s,
            Err(why) => panic!("Error, could not open tty: {}", why),
        };
        let child = match ttyserver.spawn(command) {
                               Err(why) => panic!("Couldn't spawn {}: {}", executable, why.description()),
                               Ok(p) => p,
                            };

        history.borrow_mut().push(HistoryType::Info, format!("Successfully launched {}!\n", executable));
        let fd = FileDesc::new(ttyserver.get_master().as_raw_fd(), false);
        let fd2 = fd.dup().unwrap();
        let child_stdin = unsafe { PipeWriter::from_raw_fd(fd.as_raw_fd())};
        let child_stdout = unsafe { PipeReader::from_raw_fd(fd2.as_raw_fd())};
        Child {
            child: child,
            ttyserver: ttyserver,
            stdin: child_stdin,
            stdout: child_stdout,
            history: history,
            mailbox: Vec::new(),
            foreground: foreground,
        }
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

    pub fn send(&mut self, msg:String) {
        self.mailbox.push(msg);
    }

    pub fn write(&mut self) {
        for msg in self.mailbox.drain(..) {
            let _ = self.stdin.write(msg.as_bytes());
        }
    }

    pub fn wait(&mut self) -> Result<ExitStatus, io::Error> {
        self.child.wait()
    }
}

