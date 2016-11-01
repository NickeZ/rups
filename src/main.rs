#[macro_use]
extern crate clap;

use std::error::Error;
use std::io::{self};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::process::{Command, Stdio};
use std::thread;
use std::str;

use clap::{Arg, App, SubCommand};

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
            Err(why) => panic!("Couldn't spawn cat: {}", why.description()),
            Ok(process) => process,
        };
    } else {
        process = match Command::new(command)
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .spawn() {
            Err(why) => panic!("Couldn't spawn cat: {}", why.description()),
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
}
