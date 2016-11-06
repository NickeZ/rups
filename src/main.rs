#[macro_use]
extern crate clap;
extern crate mio;

use std::error::Error;
use std::io::{self};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::process::{Command, Stdio};
use std::thread;
use std::str;
use std::collections::HashMap;

use clap::{Arg, App};

use mio::*;
use mio::tcp::{TcpListener, TcpStream};

struct TelnetServer {
    socket: TcpListener,
    clients: HashMap<Token, TcpStream>,
    token_counter: usize,
}

const TELNET_SERVER: Token = Token(0);

fn main() {
    let matches = App::new("procServ-ng")
                          .version("0.1.0")
                          .author("Niklas Claesson <nicke.claesson@gmail.com>")
                          .about("Simple process controller")
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

    if matches.is_present("foreground") {
        let stdout = process.stdout.take().unwrap();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                print!("got line: {}\n", line.unwrap())
            }
        });
    }


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


    let addr = "127.0.0.1:3000".parse().unwrap();

    let mut ts = TelnetServer {
        socket: TcpListener::bind(&addr).unwrap(),
        clients: HashMap::new(), 
        token_counter: 1
    };

    let mut events = Events::with_capacity(1_024);

    let poll = Poll::new().unwrap();

    poll.register(&ts.socket, TELNET_SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in events.iter() {
            //Handle event
            match event.token() {
                TELNET_SERVER => {
                    let client_socket = match ts.socket.accept() {
                        Err(why) => {
                            println!("Failed to accept connection: {}", why.description());
                            return;
                        },
                        //Ok((None,)) => unreachable!("Accept has returned None"),
                        Ok((stream, addr)) => stream,
                    };

                    println!("Connection on : {}", addr);

                    let new_token = Token(ts.token_counter);
                    ts.token_counter += 1;

                    ts.clients.insert(new_token, client_socket);

                },
                _ => unreachable!(),
            }
        }
    }
    //event_loop.run(&mut TelnetServer).unwrap();

}
