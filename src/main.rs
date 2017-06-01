#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate clap;
extern crate time;
extern crate rust_telnet;
extern crate pty;
extern crate fd;
#[macro_use]
extern crate chan;
extern crate chan_signal;
extern crate libc;
extern crate termios;
extern crate byteorder;
#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_signal;
extern crate tokio_timer;

mod history;
mod telnet_server;
//mod telnet_client;
mod child;
mod options;
mod send_all;

//use std::io::prelude::*;
//use std::os::unix::io::{FromRawFd};
use std::{str};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, Mutex};
use std::io;
use std::time::Duration;

use futures::{Future, Sink, Stream};
use tokio_signal::unix::Signal;

//use mio::*;
//use mio::timer::{Timer};
//use mio::deprecated::{PipeReader};
//use chan_signal::{Signal};
use termios::*;
//use log::LogLevel;

use history::*;
//use telnet_server::*;
use child::{ProcessReaders, ProcessWriters};
use options::Options;

//fn push_info(history:&Rc<RefCell<History>>, message:String) {
//    history.borrow_mut().push(HistoryLine::Info{message: message});
//}

fn main() {
    env_logger::init().unwrap();
    // Store the old termios settings, we might change them
    let mut termios = None;
    if unsafe{libc::isatty(libc::STDIN_FILENO)} == 1 {
        termios = Some(Termios::from_fd(libc::STDIN_FILENO).unwrap());
    }
    //let signal = chan_signal::notify(&[Signal::INT, Signal::TERM, Signal::CHLD]);

    let(sdone, rdone) = chan::sync(0);

    let options = Options::parse_args();

    if options.binds.is_none() && options.logbinds.is_none() {
        panic!("No network binds!");
    }

    //::std::thread::spawn(move || run(options, sdone));
    run(options, sdone);

    //chan_select! {
    //    signal.recv() -> signal => {
    //        println!("Received signal {:?}, exiting...", signal)
    //    },
    //    rdone.recv() => {
    //        println!("Program completed normally");
    //    }
    //}
    // Reset the termios after exiting
    if let Some(ref termios) = termios {
        let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, termios).unwrap();
    }
}


fn run(options: Options, _sdone: chan::Sender<()>) {
    let mut core = tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    let history = Rc::new(RefCell::new(History::new(options.history_size)));

    let timer = tokio_timer::Timer::default();

    let mut child = child::Process::new(
        options.command.clone(),
        history.clone(),
        options.foreground,
        core.handle(),
    );
    if options.autostart {
        child.spawn().expect("Failed to launch process");
    }
    let child = Arc::new(Mutex::new(child));

    let terminate = Signal::new(libc::SIGINT, &handle);
    let dead_children = Signal::new(libc::SIGCHLD, &handle);

    let sigchld_handling = dead_children.and_then(|signal| {
        println!("got stream of signals");
        signal.fold(timer, |timer, signal| {
            println!("CHILD DIED {:?}", signal);
            let child = child.clone();
            child.lock().unwrap().wait().unwrap();
            println!("CHILD reaped {:?}", signal);
            let timeout = timer.sleep(Duration::from_millis(1000))
                .and_then(move |_| {
                    child.lock().unwrap().spawn().unwrap();
                    Ok(())
                }).map(|_|()).map_err(|_|());
            handle.spawn(timeout);
            let res: Result<tokio_timer::Timer, io::Error> = Ok(timer);
            res
        }).map(|_| ())
    }).map_err(|_| unimplemented!());

    let sigint_handling = terminate.and_then(|signal| {
        println!("got stream of signals");
        signal.into_future().then(|_result| {
            println!("stahp ");
            Ok(())
        })
    }).map_err(|_|println!("error signal"));

    let child_readers = ProcessReaders::new(child.clone());
    let proc_output = child_readers
        .for_each(|reader| {
            let hw = HistoryWriter::new(history.clone());
            hw.send_all(reader).map(|_|()).or_else(|_|{
                let child = child.clone();
                println!("restart");
                //child.lock().unwrap().spawn().expect("failed to spawn..");
                Ok(())
            })
        }).map_err(|_|());

    let mut telnet_server = telnet_server::TelnetServer::new(history.clone(), child.clone(), options.noinfo);
    if let Some(binds) = options.binds {
        for bind in binds {
            telnet_server.bind(&bind, core.handle(), false);
        }
    }
    if let Some(binds) = options.logbinds {
        for bind in binds {
            telnet_server.bind(&bind, core.handle(), true);
        }
    }

    //let join = telnet_server.server(core.handle()).join(proc_output).map(|_| ());
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
    //core.run(proc_output).map(|_| ()).unwrap();
//

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
//    let mut timer = Timer::default();
//    poll.register(&timer, TIMER, Ready::readable(), PollOpt::edge()).unwrap();

//    let mut events = Events::with_capacity(1_024);
//    loop {
//        poll.poll(&mut events, None).unwrap();
//
//        for event in &events {
//            debug!("Event loop {:?}", event);
//            if event.kind().is_readable() {
//                //println!("got read token {:?}", token);
//                match event.token() {
//                    TIMER => {
//                        child = Child::new_from_child(child);
//                        child_select(&poll, &mut child);
//                    },
//                    CHILD_STDOUT => {
//                        // Read from the child process
//                        child.read();
//                        // We are ready to write data to telnet connections
//                        telnet_server.poll_clients_write(&poll);
//                        // We are also ready to get more data from the child process
//                        poll.reregister(child.stdout(), CHILD_STDOUT, Ready::readable(),
//                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                    },
//                    CHILD_STDIN => { unreachable!() },
//                    PROMPT_INPUT => {
//                        if let Some(ref mut prompt_input) = prompt_input {
//                            let mut buffer = [0;2048];
//                            let len = prompt_input.read(&mut buffer).unwrap();
//                            match String::from_utf8(buffer[0..len].to_vec()) {
//                                Err(why) => println!("Failed to parse utf8: {}", why),
//                                Ok(line) => {
//
//                                    child.send(line);
//                                    poll.reregister(child.stdin(), CHILD_STDIN, Ready::writable(),
//                                                    PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                                }
//                            };
//                            poll.reregister(prompt_input, PROMPT_INPUT, Ready::readable(),
//                                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                        }
//                    }
//                    token => {
//                        // Check if the token is a server token and accept the new connection.
//                        if telnet_server.try_accept(&poll, token, history.clone()) {
//                            continue;
//                        }
//
//                        // If something of value was recieved from the client,
//                        // send that to the child process.
//                        {
//                        let mut client = telnet_server.conn(token);
//                        let mut interest = Ready::readable();
//                        if let Some(command) = client.read() {
//                            if command.is_empty() {
//                                interest = interest | Ready::hup();
//                            } else if client.kind == BindKind::Control {
//                                debug!("read from telnetclient: {:?}", command);
//                                match command.as_ref() {
//                                    "\x12" => { // Ctrl-R
//                                        child = Child::new_from_child(child);
//                                        child_select(&poll, &mut child);
//                                    },
//                                    "\x14" => { // Ctrl-T
//                                        options.toggle_autorestart();
//                                    },
//                                    "\x18" => { // Ctrl-X
//                                        child.kill();
//                                    },
//                                    _ => {
//                                        if child.is_alive() {
//                                            child.send(command);
//                                            poll.reregister(child.stdin(), CHILD_STDIN, Ready::writable(),
//                                                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                                        }
//                                    },
//                                };
//                            }
//                        }
//                        // Register the client connection for more reading
//                        poll.reregister(client.get_stream(), token, interest,
//                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                        }
//                        let (rows, cols) = telnet_server.get_window_size();
//                        child.resize(rows, cols);
//                    }
//                }
//            }
//
//            if event.kind().is_writable() {
//                //println!("got write token {:?}", event.token());
//                match event.token() {
//                    SERVER_BIND_START | CHILD_STDOUT | PROMPT_INPUT => {},
//                    TIMER => {},
//                    CHILD_STDIN => {
//                        child.write();
//                    },
//                    token => {
//                        // Get the telnet client from the connection
//                        let mut client = telnet_server.conn(token);
//                        // Run the clients write method since there should be something to write
//                        client.write();
//                        // Reregister the client for reading.
//                        poll.reregister(client.get_stream(), token,
//                                        Ready::readable(),
//                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
//                    }
//                }
//            }
//
//            if event.kind().is_hup() {
//                match event.token() {
//                    CHILD_STDOUT | CHILD_STDIN => {
//                        // Clean out the old process
//                        poll.deregister(child.stdout()).unwrap();
//                        poll.deregister(child.stdin()).unwrap();
//                        child.kill();
//                        {
//                            let exit_status = child.wait();
//
//                            // Notify all clients that process died
//                            push_info(&history, String::from(format!("Process died with exit status {}\r\n", exit_status)));
//                            telnet_server.poll_clients_write(&poll);
//                        }
//
//                        // Create a new process
//                        if options.autorestart {
//                            if options.holdoff > 0.0 {
//                                let seconds = options.holdoff as u64;
//                                let nanos = ((options.holdoff - seconds as f64) * 1e9 ) as u32;
//                                timer.set_timeout(std::time::Duration::new(seconds, nanos), "execute").unwrap();
//                                push_info(&history, String::from(format!("Restarting in {} seconds\n", options.holdoff)));
//                                telnet_server.poll_clients_write(&poll);
//                            } else {
//                                child = Child::new_from_child(child);
//                                child_select(&poll, &mut child);
//                            }
//                        }
//                    },
//                    PROMPT_INPUT => {
//                        break;
//                    },
//                    TIMER => {},
//                    token => {
//                        // Remove the client from the slab
//                        let client = telnet_server.remove(token).unwrap();
//                        // Deregister the client from the event loop
//                        poll.deregister(client.get_stream()).unwrap();
//                        // Notify the other clients that this client is gone
//                        push_info(&history, format!("[{}] Connection lost\r\n", client.get_addr()));
//                        telnet_server.poll_clients_write(&poll);
//                    }
//                }
//            }
//        }
//    }
}
