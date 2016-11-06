#[macro_use]
extern crate clap;
extern crate mio;

use std::error::{Error};
use std::io::{self};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::process::{Command, Stdio};
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

impl History {
    fn new() -> History {
        History {
            lines: Vec::new(),
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

        self.clients.insert(new_token, TelnetClient::new(stream, addr, Ready::readable(), history));
        new_token
    }
}

struct TelnetClient {
    addr: SocketAddr,
    stream: TcpStream,
    interest: Ready,
    history: Rc<RefCell<History>>,
}

impl TelnetClient {
    fn new(stream:TcpStream, addr: SocketAddr, interest:Ready, history:Rc<RefCell<History>>) -> TelnetClient {
        TelnetClient {
            stream: stream,
            addr: addr,
            interest: interest,
            history: history,
        }
    }

    fn read(&mut self) {
        let mut buffer = [0;2048];
        match self.stream.read(&mut buffer) {
            Err(why) => println!("noo, {}", why),
            Ok(len) => {
                let content = str::from_utf8(&buffer).expect("invalid utf8");
                if len > 0 {
                    println!("{}, Got: {}", self.addr, content);
                    self.interest = Ready::writable();
                } else {
                    self.interest = self.interest | Ready::hup();
                }
            }
        }
    }

    fn write(&mut self) {
        for line in &self.history.borrow_mut().lines {
            self.stream.write(b"\x1B[34m");
            self.stream.write(line.as_bytes());
            self.stream.write(b"\x1B[0m");
        }
        self.interest = Ready::readable();
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
    let mut process;
    let (command, args) = commands.split_first().unwrap();
    if args.len() > 0 {
        process = match Command::new(command)
                                .args(&args)
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .spawn() {
            Err(why) => panic!("Couldn't spawn {}: {}", command, why.description()),
            Ok(process) => process,
        };
    } else {
        process = match Command::new(command)
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .spawn() {
            Err(why) => panic!("Couldn't spawn {}: {}", command, why.description()),
            Ok(process) => process,
        };
    }

    let mut child_stdout = PipeReader::from_stdout(process.stdout.take().unwrap()).unwrap();

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


    if matches.is_present("interactive") {
        let mut stdin = process.stdin.unwrap();
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

    let history = Rc::new(RefCell::new(History::new()));

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
                        child_stdout.read(&mut buffer);
                        let mut line = String::new();
                        line.push_str(str::from_utf8(&buffer).unwrap());
                        history.borrow_mut().lines.push(line);
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
