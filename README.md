# Rups

[![Build Status](https://travis-ci.org/NickeZ/rups.svg?branch=master)](https://travis-ci.org/NickeZ/rups)

This is an attempt to write an event-driven
[procServ(1)](https://linux.die.net/man/1/procserv) implementation in rust.

The purpose of Rups is to launch a process and then hook up the input and
outputs of it to telnet connections. It enables the connection of arbitrary
many telnet clients to the server. All clients are synchronized immediately if
anything happens to the process.

Rups also keeps your process alive by restarting it as soon as it dies.

A lot of the functionality is not implemented. This project should be
considered under development...


## Install

1. Install rust with: `curl https://sh.rustup.rs -sSf | sh`. (https://www.rustup.rs/)
2. Install rups with: `cargo install --git https://github.com/nickez/rups`
3. Run with `rups -h` to see the built-in help.

## Example Usage

1. Launch python through rups: `rups python`
2. Connect to python through a separate terminal: `telnet localhost 3000`

## Help

```
$ rups -h
Rups 0.1.0
Niklas Claesson <nicke.claesson@gmail.com>
Rust process server
USAGE:
    rups [FLAGS] [OPTIONS] <command>...
FLAGS:
    -f, --foreground       print process output to stdout (server)
    -h, --help             Prints help information
    -I, --interactive      Connect stdin to process input (server)
        --noautorestart    do not restart the child process by default
    -n, --noinfo           suppress messages (clients)
    -q, --quiet            suppress messages (server)
    -V, --version          Prints version information
    -w, --wait             let user start process via telnet command
OPTIONS:
        --autorestartcmd <autorestartcmd>    Command to toggle autorestart of process
    -b, --bind <bind>...                     Bind to address (default is 127.0.0.1:3000
    -c, --chdir <chdir>                      Process working directory
        --histsize <histsize>                Set maximum telnet packets to remember
        --holdoff <holdoff>                  wait n seconds between process restart
    -k, --killcmd <killcmd>                  Command to send SIGKILL to process
    -l, --logbind <logbind>...               Bind to address for log output (ignore any received data)
    -L, --logfile <logfile>...               Output to logfile
    -r, --restartcmd <restartcmd>            Command to start the process
ARGS:
    <command>...
All commands (killcmd, ...) take either a single letter or caret (^) + a single letter as arguments. For example '^x'
for Ctrl-X or 'x' for literal x.
EXAMPLES:
    rups bash
    Will launch bash as the child process using the default options.
```

## Demo

[![asciicast](https://asciinema.org/a/124007.png)](https://asciinema.org/a/124007)
