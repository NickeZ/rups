#[macro_use]
extern crate clap;
extern crate mio;
extern crate time;
extern crate rust_telnet as telnet;
extern crate tty;
extern crate fd;
#[macro_use]
extern crate chan;
extern crate chan_signal;
extern crate libc;
extern crate termios;

use std::error::{Error};
//use std::io::{self};
use std::io::prelude::*;
use std::io::{self};
//use std::io::{BufReader, BufWriter};
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::process::{self, Command};
use std::{str};
use std::collections::{HashMap};

use std::cell::RefCell;
use std::rc::Rc;

use std::net::{SocketAddr};

use clap::{Arg, App};

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::deprecated::{PipeReader, PipeWriter};

use telnet::parser::{TelnetTokenizer, TelnetToken};
use telnet::iac::*;

use tty::{TtyServer, FileDesc};
use chan_signal::{Signal};

use termios::*;

enum HistoryType {
    Child,
    Info,
}

struct History {
    buffers: Vec<(HistoryType, String)>,
}

impl History {
    fn new() -> History {
        History {
            buffers: Vec::new(),
        }
    }
}


#[allow(dead_code)]
struct Child {
    child: process::Child,
    ttyserver: TtyServer,
    stdin: PipeWriter,
    stdout: PipeReader,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
    foreground: bool,
}

impl Child {
    fn new(commands:&Vec<String>, history:Rc<RefCell<History>>, foreground:bool) -> Child {
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

        history.borrow_mut().buffers.push((HistoryType::Info, format!("Successfully launched {}!\n", executable)));
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

    fn read(&mut self) {
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
        self.history.borrow_mut().buffers.push((HistoryType::Child, line));
    }

    fn send(&mut self, msg:String) {
        self.mailbox.push(msg);
    }

    fn write(&mut self) {
        for msg in self.mailbox.drain(..) {
            let _ = self.stdin.write(msg.as_bytes());
        }
    }
}

struct TelnetServer {
    socket: TcpListener,
    clients: HashMap<Token, TelnetClient>,
    token_counter: usize,
}

impl TelnetServer {
    fn new(socket:TcpListener) -> TelnetServer {
        TelnetServer {
            socket: socket,
            clients: HashMap::new(),
            token_counter: TELNET_CLIENT_START.0,
        }
    }

    fn add_client(&mut self, stream:TcpStream, addr:SocketAddr, history:Rc<RefCell<History>>) -> Token {
        let new_token = Token(self.token_counter);
        self.token_counter += 1;

        self.clients.insert(new_token, TelnetClient::new(stream, addr, Ready::readable() | Ready::writable(), history));
        new_token
    }

    fn notify_clients(&mut self, poll:& mio::Poll){
        for (tok, client) in &self.clients {
            poll.reregister(&client.stream, *tok, Ready::writable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }
    }
}

#[derive(PartialEq)]
enum ClientState {
    Connected,
    HasSentMotd,
}

struct TelnetClient {
    addr: SocketAddr,
    stream: TcpStream,
    interest: Ready,
    history: Rc<RefCell<History>>,
    cursor: usize,
    state: ClientState,
    tokenizer: TelnetTokenizer,
    server_echo: bool,
}

const LINESEP:char = '\n';

impl TelnetClient {
    fn new(stream:TcpStream, addr: SocketAddr, interest:Ready, history:Rc<RefCell<History>>) -> TelnetClient {
        TelnetClient {
            stream: stream,
            addr: addr,
            interest: interest,
            history: history,
            cursor:0,
            state: ClientState::Connected,
            tokenizer: TelnetTokenizer::new(),
            server_echo: true,
        }
    }

    fn read(&mut self) -> Option<String> {
        let mut buffer = [0;2048];
        match self.stream.read(&mut buffer) {
            Err(why) => println!("Failed to read stream: {}", why),
            Ok(len) => {
                let mut content = String::new();
                for token in self.tokenizer.tokenize(&buffer[0..len]) {
                    match token {
                        TelnetToken::Text(text) => {
                            //println!("token text: {:?}", text);
                            if text[0] == '\r' as u8 {
                                content.push(LINESEP);
                            } else {
                                content.push_str(str::from_utf8(text).unwrap());
                            }
                        },
                        TelnetToken::Command(command) => {
                            println!("Command {:?}", command);
                        },
                        TelnetToken::Negotiation{command, channel} => {
                            match (command, channel) {
                                (IAC::DO, 1) => {
                                    self.server_echo = true
                                },
                                (IAC::DONT, 1) => {
                                    self.server_echo = false
                                },
                                (IAC::DO, 3) | (IAC::DONT, 3) => {},
                                _ => println!("Unsupported Negotiation {:?} {}", command, channel),
                            }
                        }
                    }
                }
                if len == 0 {
                    self.interest = self.interest | Ready::hup();
                }
                return Some(content);
            }
        }
        None
    }

    fn write_motd(&mut self) {
        let _ = self.stream.write(b"\x1B[33m");
        let _ = self.stream.write(b"Welcome to Simple Process Server 0.0.1\r\n");
        let now = time::strftime("%a, %d %b %Y %T %z", &time::now());
        let _ = self.stream.write(b"This server was started at: ");
        let _ = self.stream.write(now.unwrap().as_bytes());
        let _ = self.stream.write(b"\x1B[0m\r\n");
        let _ = self.stream.write(&[0xff, IAC::WILL, 1]);
        let _ = self.stream.write(&[0xff, IAC::WILL, 3]);
    }

    fn write(&mut self) {
        if self.state == ClientState::Connected {
            self.write_motd();
            self.state = ClientState::HasSentMotd;
        }
        for line in &self.history.borrow_mut().buffers[self.cursor..] {
            let preamble = match line.0 {
                HistoryType::Child => b"\x1B[39m",
                HistoryType::Info => b"\x1B[33m",
            };
            //let _ = self.stream.write(b"\r\n");
            let _ = self.stream.write(preamble);
            let _ = self.stream.write(line.1.replace("\n", "\r\n").as_bytes());
            let _ = self.stream.write(b"\x1B[0m");
            self.cursor += 1;
        }
        self.interest = Ready::readable();
    }
}

const TELNET_SERVER: Token = Token(0);
const CHILD_STDOUT: Token = Token(1);
const CHILD_STDIN: Token = Token(2);
const PROMPT_INPUT: Token = Token(3);
const TELNET_CLIENT_START: Token = Token(4);

pub fn to_hex_string(bytes: Vec<u8>) -> String {
    let strs: Vec<String> = bytes.iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    strs.join(" ")
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

fn push_info(history:&Rc<RefCell<History>>, message:String) {
    history.borrow_mut().buffers.push((HistoryType::Info, message));
}

fn run(_sdone: chan::Sender<()>) {
    let matches = App::new("procServ-ng")
                          .version("0.1.0")
                          .author("Niklas Claesson <nicke.claesson@gmail.com>")
                          .about("Simple process server")
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
                               .help("Bind to address")
                               .takes_value(true))
                          .arg(Arg::with_name("logbind")
                               .short("l")
                               .long("logbind")
                               .multiple(true)
                               .help("Bind to address (restrict to logging)")
                               .takes_value(true))
                          .arg(Arg::with_name("logfile")
                               .short("L")
                               .long("logfile")
                               .multiple(true)
                               .help("Bind to address (restrict to logging)")
                               .takes_value(true))
                          .arg(Arg::with_name("command")
                               .required(true)
                               .multiple(true))
                          .get_matches();

    let commands = values_t!(matches, "command", String).unwrap();
    let foreground = matches.is_present("foreground");

    let history = Rc::new(RefCell::new(History::new()));

    let mut child = Child::new(&commands, history.clone(), foreground);



    let addr = "127.0.0.1:3000".parse().unwrap();

    let tcp_listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(why) => panic!("Failed to bind to port {}", why),
    };

    let mut telnet_server = TelnetServer::new(tcp_listener);

    let mut events = Events::with_capacity(1_024);

    let poll = Poll::new().unwrap();

    poll.register(&telnet_server.socket, TELNET_SERVER,
                  Ready::readable(), PollOpt::edge()).unwrap();

    poll.register(&child.stdin, CHILD_STDIN, Ready::writable(),
                  PollOpt::edge() | PollOpt::oneshot()).unwrap();

    poll.register(&child.stdout, CHILD_STDOUT, Ready::readable(),
                  PollOpt::edge() | PollOpt::oneshot()).unwrap();

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

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.kind().is_readable() {
                match event.token() {
                    TELNET_SERVER => {
                        let (client_stream, client_addr)  = match telnet_server.socket.accept() {
                            Err(why) => {
                                println!("Failed to accept connection: {}", why.description());
                                break;
                            },
                            Ok((stream, addr)) => {
                                push_info(&history, format!("[{}] Connection established\n", addr));
                                telnet_server.notify_clients(&poll);
                                (stream, addr)
                            },
                        };

                        let new_token = telnet_server.add_client(client_stream, client_addr, history.clone());
                        let stream = &telnet_server.clients[&new_token].stream;
                        poll.register(stream, new_token, Ready::readable() | Ready::writable(),
                                      PollOpt::edge() | PollOpt::oneshot()).unwrap();

                    },
                    CHILD_STDOUT => {
                        // Read from the child process
                        child.read();
                        // We are ready to write data to telnet connections
                        telnet_server.notify_clients(&poll);
                        // We are also ready to get more data from the child process
                        poll.reregister(&child.stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
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
                                    poll.reregister(&child.stdin, CHILD_STDIN, Ready::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                                }
                            };
                            poll.reregister(prompt_input, PROMPT_INPUT, Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        }
                    }
                    token => {
                        // Get the telnet client from the collection
                        let mut client = telnet_server.clients.get_mut(&token).unwrap();
                        // Register the client connection for more reading
                        poll.reregister(&client.stream, token, client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        // If the client sent something of value, send that to the process
                        // Register to the event loop since we are ready to write some data to the child process.
                        if let Some(command) = client.read() {
                            child.send(command);
                            poll.reregister(&child.stdin, CHILD_STDIN, Ready::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        }
                    }
                }
            }

            if event.kind().is_writable() {
                match event.token() {
                    TELNET_SERVER | CHILD_STDOUT | PROMPT_INPUT => {},
                    CHILD_STDIN => {
                        child.write();
                    },
                    token => {
                        // Get the telnet client from the connection
                        let mut client = telnet_server.clients.get_mut(&token).unwrap();
                        // Run the clients write method since there should be something to write
                        client.write();
                        // Reregister the client for reading.
                        poll.reregister(&client.stream, token,
                                        client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_hup() {
                match event.token() {
                    CHILD_STDOUT | CHILD_STDIN => {
                        // Notify all clients that process died
                        push_info(&history, String::from("Process died, restarting...\n"));
                        telnet_server.notify_clients(&poll);

                        // Clean out the old process
                        poll.deregister(&child.stdout).unwrap();
                        poll.deregister(&child.stdin).unwrap();
                        child.child.wait().expect("Failed to wait on child");

                        // Create a new process
                        child = Child::new(&commands, history.clone(), foreground);
                        poll.register(&child.stdin, CHILD_STDIN, Ready::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        poll.register(&child.stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    },
                    PROMPT_INPUT => {
                        break;
                    },
                    token => {
                        let client = telnet_server.clients.remove(&token).unwrap();
                        push_info(&history, format!("[{}] Connection lost\n", client.addr));
                        telnet_server.notify_clients(&poll);
                        poll.deregister(&client.stream).unwrap();
                    }
                }
            }
        }
    }
}
