#[macro_use]
extern crate clap;
extern crate mio;

use std::error::{Error};
use std::io::{self};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::process::{self, Command, Stdio};
use std::{thread};
use std::{str};
use std::collections::{HashMap};

use std::cell::RefCell;
use std::rc::Rc;

use std::net::{SocketAddr};

use clap::{Arg, App};

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::deprecated::{PipeReader, PipeWriter};

enum HistoryLineType {
    Stdout,
    Stderr,
    Info,
}


struct History {
    lines: Vec<(HistoryLineType, String)>,
}

impl History {
    fn new() -> History {
        History {
            lines: Vec::new(),
        }
    }
}


struct Child {
    child: process::Child,
    alive: bool,
    history: Rc<RefCell<History>>,
    mailbox: Vec<String>,
}

impl Child {
    fn new(commands:&Vec<String>, history:Rc<RefCell<History>>) -> Child {
        let (executable, args) = commands.split_first().unwrap();
        let mut command = Command::new(executable);
        if args.len() > 0 {
            command.args(&args);
        }
        let child = match command.stdin(Stdio::piped())
                           .stdout(Stdio::piped())
                           .spawn() {
                               Err(why) => panic!("Couldn't spawn {}: {}", executable, why.description()),
                               Ok(process) => process,
                            };
        println!("Successfully launched {}!", executable);
        Child {
            child: child,
            alive: true,
            history: history,
            mailbox: Vec::new(),
        }
    }

    fn send(&mut self, msg:String) {
        self.mailbox.push(msg);
    }

    fn write(&mut self) {
        for msg in self.mailbox.drain(..) {
            self.child.stdin.unwrap().write(msg.as_bytes());
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
            token_counter: 3,
        }
    }

    fn add_client(&mut self, stream:TcpStream, addr:SocketAddr, history:Rc<RefCell<History>>) -> Token {
        let new_token = Token(self.token_counter);
        self.token_counter += 1;

        self.clients.insert(new_token, TelnetClient::new(stream, addr, Ready::readable() | Ready::writable(), history));
        new_token
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
    state: ClientState
}

impl TelnetClient {
    fn new(stream:TcpStream, addr: SocketAddr, interest:Ready, history:Rc<RefCell<History>>) -> TelnetClient {
        TelnetClient {
            stream: stream,
            addr: addr,
            interest: interest,
            history: history,
            cursor:0,
            state: ClientState::Connected,
        }
    }

    fn read(&mut self) -> Option<String> {
        let mut buffer = [0;2048];
        match self.stream.read(&mut buffer) {
            Err(why) => println!("noo, {}", why),
            Ok(len) => {
                let content:String = match String::from_utf8(buffer[0..len].to_vec()) {
                    Err(why) => {
                        println!("Got non-utf8.. {}", why);
                        String::new()
                    },
                    Ok(content) => content,
                };
                if len > 0 {
                    println!("{}, Got: {}", self.addr, content);
                    self.interest = Ready::writable();
                } else {
                    self.interest = self.interest | Ready::hup();
                }
                return Some(content);
            }
        }
        None
    }

    fn write_motd(&mut self) {
        self.stream.write(b"\x1B[33m");
        self.stream.write(b"Welcome to Simple Process Server");
        self.stream.write(b"\x1B[0m\r\n");
    }

    fn write(&mut self) {
        if self.state == ClientState::Connected {
            self.write_motd();
            self.state = ClientState::HasSentMotd;
        }
        for line in &self.history.borrow_mut().lines[self.cursor..] {
            let preamble = match line.0 {
                HistoryLineType::Stdout => b"\x1B[39m",
                HistoryLineType::Stderr => b"\x1B[32m",
                HistoryLineType::Info => b"\x1B[33m",
            };
            self.stream.write(preamble);
            self.stream.write(line.1.as_bytes());
            self.stream.write(b"\x1B[0m");
            self.stream.write(b"\r\n");
            self.cursor += 1;
        }
        self.interest = Ready::readable();
        /*
        if self.child.borrow_mut().alive == false {
            self.stream.write(b"\x1B[34m");
            self.stream.write(b"Process has died...\n");
            self.stream.write(b"\x1B[0m");
        }
        */
    }
}

const TELNET_SERVER: Token = Token(0);
const CHILD_STDOUT: Token = Token(1);
const CHILD_STDERR: Token = Token(2);
const CHILD_STDIN: Token = Token(3);
const TELNET_CLIENT_START: Token = Token(4);

fn main() {
    let matches = App::new("procServ-ng")
                          .version("0.1.0")
                          .author("Niklas Claesson <nicke.claesson@gmail.com>")
                          .about("Simple process server")
                          .arg(Arg::with_name("quiet")
                               .short("q")
                               .long("quiet"))
                          .arg(Arg::with_name("foreground")
                               .short("f")
                               .long("foreground"))
                          .arg(Arg::with_name("holdoff")
                               .long("holdoff"))
                          .arg(Arg::with_name("interactive")
                               .short("I")
                               .long("interactive"))
                          .arg(Arg::with_name("port")
                               .required(true))
                          .arg(Arg::with_name("command")
                               .required(true)
                               .multiple(true))
                          .get_matches();

    let commands = values_t!(matches, "command", String).unwrap();

    let history = Rc::new(RefCell::new(History::new()));

    let mut child = Child::new(&commands, history.clone());

    let mut child_stdout = PipeReader::from_stdout(child.child.stdout.take().unwrap()).unwrap();
    let mut child_stdin = PipeWriter::from_stdin(child.child.stdin.take().unwrap()).unwrap();

        /*
    if matches.is_present("foreground") {
        let stdout = process.stdout.take().unwrap();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                print!("got line: {}\n", line.unwrap())
            }
        });
    }
    */


    /*
    if matches.is_present("interactive") {
        let mut stdin = child.borrow_mut().child.stdin.unwrap();
        let mut writer = BufWriter::new(&mut stdin);
        loop {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer);
            match writer.write_all(buffer.as_bytes()) {
                Err(why) => panic!("Couldn't write to process: {}",
                                why.description()),
                Ok(_) => println!("Sent to process..."),
            };
            writer.flush().unwrap();
        }
    }
    */

    let addr = "127.0.0.1:3000".parse().unwrap();

    let mut telnet_server = TelnetServer::new(TcpListener::bind(&addr).unwrap());

    let mut events = Events::with_capacity(1_024);

    let poll = Poll::new().unwrap();

    poll.register(&telnet_server.socket, TELNET_SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    poll.register(&child_stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge()).unwrap();

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
                                println!("Connection from : {}", addr);
                                (stream, addr)
                            },
                        };

                        let new_token = telnet_server.add_client(client_stream, client_addr, history.clone());

                        poll.register(&telnet_server.clients[&new_token].stream,
                                      new_token, telnet_server.clients[&new_token].interest,
                                      PollOpt::edge() | PollOpt::oneshot()).unwrap();

                    },
                    CHILD_STDOUT => {
                        let mut buffer = [0;2048];
                        let len = child_stdout.read(&mut buffer).unwrap();
                        let mut line = String::from_utf8(buffer[0..len-1].to_vec()).unwrap();
                        println!("len {}, {}", line.len(), &line);
                        history.borrow_mut().lines.push((HistoryLineType::Stdout, line));
                        for (tok, client) in &telnet_server.clients {
                            poll.reregister(&client.stream, *tok, Ready::writable(),
                                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        }
                    },
                    CHILD_STDERR => {
                    },
                    token => {
                        let mut client = telnet_server.clients.get_mut(&token).unwrap();
                        let command = client.read();
                        poll.reregister(&client.stream, token,
                                        client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        child.send(command.unwrap());
                        poll.register(&child_stdin, CHILD_STDIN, Ready::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_writable() {
                match event.token() {
                    CHILD_STDOUT => {
                    },
                    CHILD_STDERR => {
                        child.write();
                        poll.register(&child_stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge()).unwrap();
                    },
                    token => {
                        let mut client = telnet_server.clients.get_mut(&token).unwrap();
                        client.write();
                        poll.reregister(&client.stream, token,
                                        client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_hup() {
                match event.token() {
                    CHILD_STDOUT => {
                        println!("Nothing more..");
                        history.borrow_mut().lines.push((HistoryLineType::Info, String::from("Process died, restarting...")));
                        poll.deregister(&child_stdout).unwrap();
                        //child.borrow_mut().alive = false;
                        let mut new_child = Child::new(&commands, history.clone());
                        child_stdout = PipeReader::from_stdout(new_child.child.stdout.take().unwrap()).unwrap();
                        poll.register(&child_stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge()).unwrap();
                    },
                    CHILD_STDERR => {
                    },
                    token => {
                        println!("connection closed");
                        let client = telnet_server.clients.remove(&token).unwrap();
                        poll.deregister(&client.stream).unwrap();
                    }
                }
            }
        }
    }
}
