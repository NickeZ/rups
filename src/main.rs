#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate clap;
extern crate time;
extern crate rust_telnet;
extern crate pty;
extern crate fd;
extern crate libc;
extern crate termios;
extern crate byteorder;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_signal;
extern crate tokio_timer;
extern crate futures_addition;

mod history;
mod telnet_server;
mod child;
mod options;

use std::{str};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, Mutex};
use std::io;
use std::time::Duration;

use futures::{Future, Sink, Stream};
use tokio_signal::unix::Signal;

use termios::*;

use history::*;
use child::ProcessReaders;
use options::Options;

fn main() {
    env_logger::init().unwrap();
    // Store the old termios settings, we might change them
    let mut termios = None;
    if unsafe{libc::isatty(libc::STDIN_FILENO)} == 1 {
        termios = Some(Termios::from_fd(libc::STDIN_FILENO).unwrap());
    }

    let options = Options::parse_args();

    if options.binds.is_none() && options.logbinds.is_none() {
        panic!("No network binds!");
    }

    run(options);

    // Reset the termios after exiting
    if let Some(ref termios) = termios {
        let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, termios).unwrap();
    }
}


fn run(options: Options) {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    let options = Rc::new(RefCell::new(options));
    let history = Rc::new(RefCell::new(History::new(options.borrow().history_size)));

    let timer = tokio_timer::Timer::default();

    let mut child = child::Process::new(
        options.borrow().command.clone(),
        history.clone(),
        options.borrow().foreground,
        core.handle(),
    );
    if options.borrow().autostart {
        child.spawn().expect("Failed to start process");
    }
    let child = Arc::new(Mutex::new(child));

    let terminate = Signal::new(libc::SIGINT, &handle);
    let dead_children = Signal::new(libc::SIGCHLD, &handle);

    let holdoff = options.borrow().holdoff;
    let sec = holdoff.floor();
    let nsec = (holdoff - sec) * 1_000_000_000f64;

    let sigchld_handling = dead_children.and_then(|signal| {
        signal.fold(timer, |timer, signal| {
            trace!("got signal {:?}", signal);
            let child = child.clone();
            child.lock().unwrap().wait().unwrap();
            if options.borrow().autorestart {
                println!("Subprocess died, will restart in {:.2}s", holdoff);
                let timeout = timer.sleep(Duration::new(sec as u64, nsec as u32))
                    .and_then(move |_| {
                        child.lock().unwrap().spawn().unwrap();
                        Ok(())
                    }).map(|_|()).map_err(|_|());
                handle.spawn(timeout);
            }
            let res: Result<tokio_timer::Timer, io::Error> = Ok(timer);
            res
        }).map(|_| ())
    }).map_err(|_| unimplemented!());

    let sigint_handling = terminate.and_then(|signal| {
        signal.into_future().then(|_result| {
            debug!("stahp ");
            Ok(())
        })
    }).map_err(|_| unimplemented!());

    let child_readers = ProcessReaders::new(child.clone());
    let proc_output = child_readers
        .for_each(|reader| {
            let hw = HistoryWriter::new(history.clone());
            hw.send_all(reader).map(|_|()).or_else(|_|{
                Ok(())
            })
        }).map_err(|_|());

    let mut telnet_server = telnet_server::TelnetServer::new(history.clone(), child.clone(), options.clone());
    if let Some(binds) = options.borrow().binds.as_ref() {
        for bind in binds {
            telnet_server.bind(&bind, core.handle(), false);
        }
    }
    if let Some(binds) = options.borrow().logbinds.as_ref() {
        for bind in binds {
            telnet_server.bind(&bind, core.handle(), true);
        }
    }

    let telnet_server = telnet_server.server(core.handle());

    let join = futures::future::join_all(vec![
        Box::new(sigchld_handling) as Box<Future<Item=(), Error=()>>,
        Box::new(proc_output) as Box<Future<Item=(), Error=()>>,
        telnet_server,
    ]).map(|_| ());

    let select = futures::future::select_all(vec![
        Box::new(join) as Box<Future<Item=(), Error=()>>,
        Box::new(sigint_handling) as Box<Future<Item=(), Error=()>>,
    ]).map(|_| ());


    match core.run(select) {
        _ => println!("Done"),
    };

//    let mut prompt_input:Option<PipeReader> = None;
//    if options.interactive {
//        let old_termios = Termios::from_fd(libc::STDIN_FILENO).unwrap();
//        let mut new_termios = old_termios;
//        new_termios.c_lflag &= !(ICANON | ECHO);
//        let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, &mut new_termios);
//        prompt_input = Some(unsafe { PipeReader::from_raw_fd(libc::STDIN_FILENO)});
//        match prompt_input {
//            Some(ref prompt_input) => {
//                poll.register(prompt_input, PROMPT_INPUT, Ready::readable(),
//                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
//            },
//            None => {},
//        }
//    }
//

}
