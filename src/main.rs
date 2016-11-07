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

struct History {
    lines: Vec<String>,
}


struct Child {
    child: process::Child,
    alive: bool,
    history: Rc<RefCell<History>>,
}

impl Child {
    fn new(commands:&Vec<String>, history:Rc<RefCell<History>>) -> Child {
        let (executable, args) = commands.split_first().unwrap();
        let command = match Command::new(executable);
        if args.len() > 0 {
            command.args(&args)
        }
        let child = command.stdin(Stdio::piped())
                           .stdout(Stdio::piped())
                           .spawn() {
                               Err(why) => panic!("Couldn't spawn {}: {}", command, why.description()),
                               Ok(process) => process,
                            };
        }
        Child {
            child: child,
            alive: true,
            history: history,
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

    fn add_client(&mut self, stream:TcpStream, addr:SocketAddr, child:Rc<RefCell<Child>>) -> Token {
        let new_token = Token(self.token_counter);
        self.token_counter += 1;

        self.clients.insert(new_token, TelnetClient::new(stream, addr, Ready::readable() | Ready::writable(), child));
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
    child: Rc<RefCell<Child>>,
    cursor: usize,
    state: ClientState
}

impl TelnetClient {
    fn new(stream:TcpStream, addr: SocketAddr, interest:Ready, child:Rc<RefCell<Child>>) -> TelnetClient {
        TelnetClient {
            stream: stream,
            addr: addr,
            interest: interest,
            child: child,
            cursor:0,
            state: ClientState::Connected,
        }
    }

    fn read(&mut self) {
        let mut buffer = [0;2048];
        match self.stream.read(&mut buffer) {
            Err(why) => println!("noo, {}", why),
            Ok(len) => {
                let content = match str::from_utf8(&buffer) {
                    Err(why) => {
                        println!("Got non-utf8.. {}", why);
                        ""
                    },
                    Ok(content) => content,
                };
                if len > 0 {
                    println!("{}, Got: {}", self.addr, content);
                    self.interest = Ready::writable();
                } else {
                    self.interest = self.interest | Ready::hup();
                }
            }
        }
    }

    fn write_motd(&mut self) {
        self.stream.write(b"\x1B[34m");
        self.stream.write(b"Welcome to Simple Process Server\n");
        self.stream.write(b"\x1B[0m");
    }

    fn write(&mut self) {
        if self.state == ClientState::Connected {
            self.write_motd();
            self.state = ClientState::HasSentMotd;
        }
        for line in &self.child.borrow_mut().history[self.cursor..] {
            self.stream.write(b"\x1B[31m");
            self.stream.write(line.as_bytes());
            self.stream.write(b"\x1B[0m");
            self.cursor += 1;
        }
        self.interest = Ready::readable();
        if self.child.borrow_mut().alive == false {
            self.stream.write(b"\x1B[34m");
            self.stream.write(b"Process has died...\n");
            self.stream.write(b"\x1B[0m");
        }
    }
}

const TELNET_SERVER: Token = Token(0);
const CHILD_STDOUT: Token = Token(1);
const CHILD_STDERR: Token = Token(2);

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

    let mut child = Rc::new(RefCell::new(Child::new(commands)));

    let mut child_stdout = PipeReader::from_stdout(child.borrow_mut().child.stdout.take().unwrap()).unwrap();

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

                        let new_token = telnet_server.add_client(client_stream, client_addr, child.clone());

                        poll.register(&telnet_server.clients[&new_token].stream,
                                      new_token, telnet_server.clients[&new_token].interest,
                                      PollOpt::edge() | PollOpt::oneshot()).unwrap();

                    },
                    CHILD_STDOUT => {
                        let mut buffer = [0;2048];
                        child_stdout.read(&mut buffer);
                        let mut line = String::new();
                        line.push_str(str::from_utf8(&buffer).unwrap());
                        child.borrow_mut().history.push(line);
                        let from_process_s = str::from_utf8(&buffer).unwrap();
                        println!("it says something, {}", from_process_s);
                        for (tok, client) in &telnet_server.clients {
                            poll.reregister(&client.stream, *tok, Ready::writable(),
                                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
                        }
                    },
                    CHILD_STDERR => {
                    },
                    token => {
                        let mut client = telnet_server.clients.get_mut(&token).unwrap();
                        client.read();
                        poll.reregister(&client.stream, token,
                                        client.interest,
                                        PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    }
                }
            }

            if event.kind().is_writable() {
                match event.token() {
                    CHILD_STDOUT => {
                    },
                    CHILD_STDERR => {
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
                        poll.deregister(&child_stdout).unwrap();
                        //child.borrow_mut().alive = false;
                        let new_child = Child::new(commands, history);
                        let new_child_stdout = PipeReader::from_stdout(new_child.borrow_mut().child.stdout.take().unwrap()).unwrap();
                        poll.reregister(&new_child_stdout, CHILD_STDOUT, Ready::readable(), PollOpt::edge()).unwrap();
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
