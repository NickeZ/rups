#[macro_use]
extern crate clap;
extern crate mio;
extern crate slab;
extern crate time;
extern crate rust_telnet as telnet;
extern crate tty;
extern crate fd;
#[macro_use]
extern crate chan;
extern crate chan_signal;
extern crate libc;
extern crate termios;

mod history;
mod telnet_server;
mod telnet_client;
mod child;

use std::io::prelude::*;
use std::os::unix::io::{FromRawFd};
use std::net::{SocketAddr, IpAddr};
use std::{str};
use std::cell::{RefCell};
use std::rc::{Rc};
use std::str::{FromStr};

use clap::{Arg, App};
use mio::*;
use mio::timer::{Timer};
use mio::deprecated::{PipeReader};
use chan_signal::{Signal};
use termios::*;

use history::*;
use telnet_server::*;
use child::Child;

// Number of connections cannot go above 10 million.
const TIMER: Token = Token(10_000_000);
const CHILD_STDOUT: Token = Token(10_000_001);
const CHILD_STDIN: Token = Token(10_000_002);
const PROMPT_INPUT: Token = Token(10_000_003);
const SERVER_BIND_START: Token = Token(10_000_004);

struct RupsApp {
    noautorestart: bool,
    holdoff: u64,
    autostart: bool,
}

impl RupsApp {
    fn new(noautorestart: bool, holdoff: u64, autostart: bool) -> RupsApp {
        RupsApp {
            noautorestart: noautorestart,
            holdoff: holdoff,
            autostart: autostart,
        }
    }
}

fn push_info(history:&Rc<RefCell<History>>, message:String) {
    history.borrow_mut().push(HistoryType::Info, message);
}

fn child_select(poll:&Poll, child:&mut Child) {
    poll.register(child.stdin(), CHILD_STDIN, Ready::writable(),
                  PollOpt::edge() | PollOpt::oneshot()).unwrap();
    poll.register(child.stdout(), CHILD_STDOUT, Ready::readable(),
                  PollOpt::edge() | PollOpt::oneshot()).unwrap();
}

fn main() {
    // Store the old termios settings, we might change them
    let termios = Termios::from_fd(libc::STDIN_FILENO).unwrap();
    let signal = chan_signal::notify(&[Signal::INT, Signal::TERM]);

    let(sdone, rdone) = chan::sync(0);

    ::std::thread::spawn(move || run(sdone));

    chan_select! {
        signal.recv() -> signal => {
            println!("Received signal {:?}, exiting...", signal)
        },
        rdone.recv() => {
            println!("Program completed normally");
        }
    }
    // Reset the termios after exiting
    let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, &termios).unwrap();
}

fn run(_sdone: chan::Sender<()>) {
    let matches = App::new("Rups")
                          .version("0.1.0")
                          .author("Niklas Claesson <nicke.claesson@gmail.com>")
                          .about("Rust process server")
                          .arg(Arg::with_name("wait")
                               .long("wait")
                               .short("w")
                               .help("let user start process via telnet command"))
                          .arg(Arg::with_name("noautorestart")
                               .long("noautorestart")
                               .help("do not restart the child process by default"))
                          .arg(Arg::with_name("quiet")
                               .short("q")
                               .long("quiet")
                               .help("suppress messages (server)"))
                          .arg(Arg::with_name("foreground")
                               .short("f")
                               .long("foreground")
                               .help("print process output to stdout (server)"))
                          .arg(Arg::with_name("holdoff")
                               .long("holdoff")
                               .help("wait n seconds between process restart")
                               .takes_value(true))
                          .arg(Arg::with_name("interactive")
                               .short("I")
                               .long("interactive")
                               .help("Connect stdin to process input (server)"))
                          .arg(Arg::with_name("bind")
                               .short("b")
                               .long("bind")
                               .multiple(true)
                               .help("Bind to address (default is 127.0.0.1:3000")
                               .takes_value(true))
                          .arg(Arg::with_name("logbind")
                               .short("l")
                               .long("logbind")
                               .multiple(true)
                               .help("Bind to address for log output (ignore any received data)")
                               .takes_value(true))
                          .arg(Arg::with_name("logfile")
                               .short("L")
                               .long("logfile")
                               .multiple(true)
                               .help("Output to logfile")
                               .takes_value(true))
                          .arg(Arg::with_name("command")
                               .required(true)
                               .multiple(true))
                          .get_matches();

    let mut commands = values_t!(matches, "command", String).unwrap();
    let foreground = matches.is_present("foreground");

    let mut app = RupsApp::new(matches.is_present("noautorestart"),
                               value_t!(matches, "holdoff", u64).unwrap_or(0),
                               !matches.is_present("wait"));

    let history = Rc::new(RefCell::new(History::new()));

    let mut child = child::Child::new(commands.remove(0), commands, history.clone(), foreground, app.autostart);

    let mut logaddrs:Vec<SocketAddr> = Vec::new();
    if let Ok(bindv) =  values_t!(matches, "logbind", String) {
        for bind in &bindv {
            if let Ok(addr) = bind.parse() {
                logaddrs.push(addr);
            } else {
                if let Ok(port) = bind.parse() {
                    logaddrs.push(SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), port))
                } else {
                    // TODO: Parse it as a unix socket instead..
                }
            }
        }
    }

    let mut addrs:Vec<SocketAddr> = Vec::new();
    if let Ok(bindv) =  values_t!(matches, "bind", String) {
        for bind in &bindv {
            if let Ok(addr) = bind.parse() {
                addrs.push(addr);
            } else {
                if let Ok(port) = bind.parse() {
                    logaddrs.push(SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), port))
                } else {
                    // TODO: Parse it as a unix socket instead..
                }
            }
        }
    }


    let mut telnet_server = telnet_server::TelnetServer::new();

    let poll = Poll::new().unwrap();

    if app.autostart {
        child_select(&poll, &mut child);
    }

    for addr in logaddrs {
        telnet_server.add_bind(&poll, addr, BindKind::Log);
        println!("Listening on Port {}", addr);
    }

    for addr in addrs {
        telnet_server.add_bind(&poll, addr, BindKind::Control);
        println!("Listening on Port {}", addr);
    }

    let mut prompt_input:Option<PipeReader> = None;
    if matches.is_present("interactive") {
        let old_termios = Termios::from_fd(libc::STDIN_FILENO).unwrap();
        let mut new_termios = old_termios;
        new_termios.c_lflag &= !(ICANON | ECHO);
        let _ = tcsetattr(libc::STDIN_FILENO, TCSANOW, &mut new_termios);
        prompt_input = Some(unsafe { PipeReader::from_raw_fd(libc::STDIN_FILENO)});
        match prompt_input {
            Some(ref prompt_input) => {
                poll.register(prompt_input, PROMPT_INPUT, Ready::readable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
            },
            None => {},
        }
    }

    let mut timer = Timer::default();
    poll.register(&timer, TIMER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(1_024);
    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.kind().is_readable() {
                //println!("got read token {:?}", token);
                match event.token() {
                    TIMER => {
                        child = Child::new_from_child(child);
                        child_select(&poll, &mut child);
                    },
                    CHILD_STDOUT => {
                        // Read from the child process
                        child.read();
                        // We are ready to write data to telnet connections
                        telnet_server.poll_clients_write(&poll);
                        // We are also ready to get more data from the child process
                        poll.reregister(child.stdout(), CHILD_STDOUT, Ready::readable(),
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    },
                    CHILD_STDIN => { unreachable!() },
                    PROMPT_INPUT => {
                        if let Some(ref mut prompt_input) = prompt_input {
                            let mut buffer = [0;2048];
                            let len = prompt_input.read(&mut buffer).unwrap();
                            match String::from_utf8(buffer[0..len].to_vec()) {
                                Err(why) => println!("Failed to parse utf8: {}", why),
                                Ok(line) => {

                                    child.send(line);
                                    poll.reregister(child.stdin(), CHILD_STDIN, Ready::writable(),
                                                    PollOpt::edge() | PollOpt::oneshot()).unwrap();
                                }
                            };
                            poll.reregister(prompt_input, PROMPT_INPUT, Ready::readable(),
                                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        }
                    }
                    token => {
                        // Check if it is a bind socket
                        if telnet_server.try_accept(&poll, token, history.clone()) {
                            continue;
                        }

                        // Get client connection from the collection
                        let mut client = telnet_server.conn(token);
                        // If the client sent something of value, send that to the child process
                        // Register to the event loop that we are ready to write to the child process
                        if let Some(command) = client.read() {
                            if client.kind == BindKind::Control {
                                println!("command: {:?}", command);
                                match command.as_ref() {
                                    "\x12" => { // Ctrl-R
                                        child = Child::new_from_child(child);
                                        child_select(&poll, &mut child);
                                    },
                                    "\x14" => { // Ctrl-T
                                        app.noautorestart = ! app.noautorestart;
                                    },
                                    "\x18" => { // Ctrl-X
                                        child.kill();
                                    },
                                    _ => {},
                                };
                                if child.is_alive() {
                                    child.send(command);
                                    poll.reregister(child.stdin(), CHILD_STDIN, Ready::writable(),
                                                    PollOpt::edge() | PollOpt::oneshot()).unwrap();
                                }
                            }
                        }
                        // Register the client connection for more reading
                        poll.reregister(client.get_stream(), token, client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_writable() {
                //println!("got write token {:?}", event.token());
                match event.token() {
                    SERVER_BIND_START | CHILD_STDOUT | PROMPT_INPUT => {},
                    TIMER => {},
                    CHILD_STDIN => {
                        child.write();
                    },
                    token => {
                        // Get the telnet client from the connection
                        let mut client = telnet_server.conn(token);
                        // Run the clients write method since there should be something to write
                        client.write();
                        // Reregister the client for reading.
                        poll.reregister(client.get_stream(), token,
                                        client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_hup() {
                match event.token() {
                    CHILD_STDOUT | CHILD_STDIN => {
                        // Clean out the old process
                        poll.deregister(child.stdout()).unwrap();
                        poll.deregister(child.stdin()).unwrap();
                        child.kill();
                        {
                            let exit_status = child.wait();

                            // Notify all clients that process died
                            push_info(&history, String::from(format!("Process died with exit status {}\n", exit_status)));
                            telnet_server.poll_clients_write(&poll);
                        }

                        // Create a new process
                        if ! app.noautorestart {
                            if app.holdoff > 0 {
                                timer.set_timeout(std::time::Duration::from_secs(app.holdoff), "execute").unwrap();
                                push_info(&history, String::from(format!("Restarting in {} seconds\n", app.holdoff)));
                                telnet_server.poll_clients_write(&poll);
                            } else {
                                child = Child::new_from_child(child);
                                child_select(&poll, &mut child);
                            }
                        }
                    },
                    PROMPT_INPUT => {
                        break;
                    },
                    TIMER => {},
                    token => {
                        let client = telnet_server.remove(token).unwrap();
                        push_info(&history, format!("[{}] Connection lost\n", client.get_addr()));
                        telnet_server.poll_clients_write(&poll);
                        poll.deregister(client.get_stream()).unwrap();
                    }
                }
            }
        }
    }
}
