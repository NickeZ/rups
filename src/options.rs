use std::net::{SocketAddr, IpAddr};
use std::str::{FromStr};

use clap::{Arg, App};

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub struct Options {
    pub command: Vec<String>,
    pub history_size: usize,
    pub foreground: bool,
    pub interactive: bool,
    pub autostart: bool,
    pub autorestart: bool,
    pub noinfo: bool,
    pub holdoff: f64,
    pub binds: Option<Vec<SocketAddr>>,
    pub logbinds: Option<Vec<SocketAddr>>,
    pub killcmd: u8,
    pub togglecmd: u8,
    pub restartcmd: u8,
}

impl Default for Options {
    fn default() -> Options {
        let mut addrs = Vec::new();
        addrs.push(SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 3000));
        let mut logaddrs = Vec::new();
        logaddrs.push(SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 4000));
        Options {
            command: Vec::new(),
            history_size: 20_000,
            foreground: false,
            interactive: false,
            autostart: true,
            autorestart: true,
            noinfo: false,
            holdoff: 5.0,
            binds: Some(addrs),
            logbinds: Some(logaddrs),
            killcmd: 0x18,
            togglecmd: 0x14,
            restartcmd: 0x12,
        }
    }
}

impl Options {
    pub fn parse_args() -> Options {
        let mut options = Options::default();
        let matches = App::new("Rups")
            .version(VERSION)
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
            .arg(Arg::with_name("noinfo")
                .short("n")
                .long("noinfo")
                .help("suppress messages (clients)"))
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
            .arg(Arg::with_name("histsize")
                .long("histsize")
                .help("Set maximum telnet packets to remember")
                .takes_value(true))
            .arg(Arg::with_name("killcmd")
                .short("k")
                .long("killcmd")
                .help("Command to send SIGKILL to process")
                .takes_value(true))
            .arg(Arg::with_name("autorestartcmd")
                .long("autorestartcmd")
                .help("Command to toggle autorestart of process")
                .takes_value(true))
            .arg(Arg::with_name("restartcmd")
                .short("r")
                .long("restartcmd")
                .help("Command to start the process")
                .takes_value(true))
            .arg(Arg::with_name("command")
                .required(true)
                .multiple(true))
            .get_matches();

        options.command = matches.values_of("command")
            .expect("Argument 'command' missing")
            .map(String::from)
            .collect();

        options.foreground = matches.is_present("foreground");
        options.autorestart = !matches.is_present("noautorestart");
        options.autostart = !matches.is_present("wait");
        options.interactive = matches.is_present("interactive");
        options.noinfo = matches.is_present("noinfo");

        if let Ok(holdoff) = value_t!(matches, "holdoff", f64) {
            options.holdoff = holdoff;
        }
        if let Ok(history_size) = value_t!(matches, "histsize", usize) {
            options.history_size = history_size;
        }
        if let Some(bindv) = matches.values_of("bind") {
            // TODO(nc): Interpret ip:port, port, unix socket
            let bindv = bindv.collect::<Vec<&str>>();
            options.binds = Some(bindv.iter().map(|b| b.parse().unwrap()).collect());
        }
        if let Some(bindv) = matches.values_of("logbind") {
            // TODO(nc): Interpret ip:port, port, unix socket
            let bindv = bindv.collect::<Vec<&str>>();
            options.logbinds = Some(bindv.iter().map(|b| b.parse().unwrap()).collect());
        }
        if let Some(cmd) = matches.value_of("killcmd") {
            match parse_shortcut(cmd.as_bytes()) {
                Ok(cmd) => options.killcmd = cmd,
                Err(..) => println!("Failed to parse {}", cmd),
            }
        }
        if let Some(cmd) = matches.value_of("autorestartcmd") {
            match parse_shortcut(cmd.as_bytes()) {
                Ok(cmd) => options.togglecmd = cmd,
                Err(..) => println!("Failed to parse {}", cmd),
            }
        }
        if let Some(cmd) = matches.value_of("restartcmd") {
            match parse_shortcut(cmd.as_bytes()) {
                Ok(cmd) => options.restartcmd = cmd,
                Err(..) => println!("Failed to parse {}", cmd),
            }
        }

        if options.killcmd == options.togglecmd || options.killcmd == options.restartcmd || options.togglecmd == options.restartcmd {
            panic!("It is not allowed to have the same shortcut for multiple commands");
        }

        options
    }

    pub fn toggle_autorestart(&mut self) {
        self.autorestart = ! self.autorestart;
    }
}

// Parses ^[a-zA-Z] to the correct control code
fn parse_shortcut(buf: &[u8]) -> Result<u8,()> {
    match buf.len() {
        2  if buf[0] == b'^' && buf[1] >= b'A' && buf[1] <= b'z' => Ok(0x1f & buf[1]),
        1  if buf[0] >= b'A' && buf[0] <= b'z' => Ok(buf[0]),
        _ => Err(()),
    }
}
