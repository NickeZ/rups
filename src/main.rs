#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate clap;
extern crate byteorder;
extern crate fd;
extern crate futures;
extern crate futures_addition;
extern crate libc;
extern crate pty;
extern crate rust_telnet;
extern crate termios;
extern crate time;
extern crate tokio_core;
extern crate tokio_file_unix;
extern crate tokio_io;
extern crate tokio_signal;
extern crate tokio_timer;

mod child;
mod history;
mod options;
mod telnet_server;
mod util;

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{Future, Sink, Stream};
use tokio_io::io::write_all;
use tokio_signal::unix::Signal;

use pty::PtyStreamError;

use termios::*;

use child::{ProcessError, ProcessReaders};
use history::*;
use options::Options;

fn main() {
    env_logger::init().unwrap();
    // Store the old termios settings, we might change them
    let mut termios = None;
    if unsafe { libc::isatty(libc::STDIN_FILENO) } == 1 {
        termios = Some(Termios::from_fd(libc::STDIN_FILENO).unwrap());
    }

    let options = Options::parse_args();

    if options.binds.is_empty() && options.logbinds.is_empty() {
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
    let history = Rc::new(RefCell::new(History::new(&options.borrow())));

    let timer = tokio_timer::Timer::default();

    let mut child = child::Process::new(
        options.borrow().command.clone(),
        options.borrow().chdir.clone(),
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

    let sigchld_handling = dead_children
        .and_then(|signal| {
            signal
                .fold(timer, |timer, signal| {
                    trace!("got signal {:?}", signal);
                    let child = child.clone();
                    let child2 = child.clone();
                    let mut child_locked = child.lock().unwrap();
                    let pid = child_locked.id().unwrap();
                    let exitcode = child_locked.wait().unwrap();
                    println!("Received SIGCHLD for {}. {}", pid, exitcode);
                    if options.borrow().autorestart {
                        println!("Will restart in {:.2}s", holdoff);
                        let timeout = timer
                            .sleep(Duration::new(sec as u64, nsec as u32))
                            .and_then(move |_| {
                                match child2.lock().unwrap().spawn() {
                                    Err(ProcessError::ProcessAlreadySpawned) => (),
                                    Err(e) => println!("{:?}", e),
                                    Ok(..) => (),
                                }
                                Ok(())
                            })
                            .map(|_| ())
                            .map_err(|_| ());
                        handle.spawn(timeout);
                    }
                    let res: Result<tokio_timer::Timer, io::Error> = Ok(timer);
                    res
                })
                .map(|_| ())
        })
        .map_err(|_| unimplemented!());

    let sigint_handling = terminate
        .and_then(|signal| {
            signal.into_future().then(|_result| {
                debug!("stahp ");
                Ok(())
            })
        })
        .map_err(|_| unimplemented!());

    let child_readers = ProcessReaders::new(child.clone());
    let proc_output = child_readers
        .for_each(|reader| {
            let hw = HistoryWriter::new(history.clone());
            hw.send_all(reader.map_err(|e| match e {
                PtyStreamError::IoError(e) => e,
                _ => io::Error::new(io::ErrorKind::Other, "oops"),
            }))
            .map(|_| ())
            .or_else(|_| Ok(()))
        })
        .map_err(|_| ());

    let mut telnet_server =
        telnet_server::TelnetServer::new(history.clone(), child.clone(), options.clone());
    for bind in options.borrow().binds.iter() {
        telnet_server.bind(&bind, core.handle(), false);
    }
    for bind in options.borrow().logbinds.iter() {
        telnet_server.bind(&bind, core.handle(), true);
    }

    let mut joins = Vec::new();

    if options.borrow().foreground {
        let hr = HistoryReader::new(history.clone());
        //let stdout = std::io::stdout();
        //let file = tokio_file_unix::StdFile(stdout.lock()); //Does not work because of borrow
        //let file = tokio_file_unix::File::new_nb(file).unwrap();
        let file = OpenOptions::new().write(true).open("/dev/stdout").unwrap();
        let file = tokio_file_unix::File::new_nb(file).unwrap();
        let file = file.into_io(&handle).unwrap();
        let hr = hr
            .fold(file, |file, msg| write_all(file, msg).map(|(file, _)| file))
            .map(|_| ())
            .map_err(|_| ());
        joins.push(Box::new(hr) as Box<Future<Item = (), Error = ()>>);
    }

    if options.borrow().interactive {
        let old_termios = Termios::from_fd(libc::STDIN_FILENO).unwrap();
        let mut new_termios = old_termios;
        new_termios.c_lflag &= !(ICANON | ECHO);
        let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, &mut new_termios);
        //let stdin = std::io::stdin();
        //let file = tokio_file_unix::StdFile(stdin.lock()); //Does not work because of borrow
        let file = std::fs::File::open("/dev/stdin").unwrap();
        let file = tokio_file_unix::File::new_nb(file).unwrap();
        let file = file.into_io(&handle).unwrap();
        let tx = telnet_server.tx();
        let hw = futures::future::loop_fn((file, tx), |(file, tx)| {
            tokio_io::io::read(file, [0u8; 10]).and_then(|(file, buf, len)| {
                if len == 0 {
                    //return futures::future::Loop::Break(())
                    unreachable!()
                }
                tx.send(buf[0..len].to_vec())
                    .and_then(|tx| {
                        tx.flush()
                            .and_then(|tx| Ok(futures::future::Loop::Continue((file, tx))))
                    })
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "mupp"))
            })
        })
        .map_err(|_| ());
        joins.push(Box::new(hw) as Box<Future<Item = (), Error = ()>>);
    }

    let telnet_server = telnet_server.server(core.handle());

    joins.push(Box::new(sigchld_handling) as Box<Future<Item = (), Error = ()>>);
    joins.push(Box::new(proc_output) as Box<Future<Item = (), Error = ()>>);
    joins.push(telnet_server);

    let join = futures::future::join_all(joins).map(|_| ());

    let select = futures::future::select_all(vec![
        Box::new(join) as Box<Future<Item = (), Error = ()>>,
        Box::new(sigint_handling) as Box<Future<Item = (), Error = ()>>,
    ])
    .map(|_| ());

    match core.run(select) {
        _ => println!("Done"),
    };
}
