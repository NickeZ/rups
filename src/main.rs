use std::error::Error;
use std::io::{self};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::process::{Command, Stdio};
use std::thread;
use std::str;

fn main() {
    println!("Hello, world!");
    let mut process = match Command::new("cat")
                                 .stdin(Stdio::piped())
                                 .stdout(Stdio::piped())
                                 .spawn() {
        Err(why) => panic!("Couldn't spawn cat: {}", why.description()),
        Ok(process) => process,
    };

    let stdout = process.stdout.take().unwrap();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            print!("got line: {}\n", line.unwrap())
        }
    });


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
