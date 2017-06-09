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

[![asciicast](https://asciinema.org/a/124007.png)](https://asciinema.org/a/124007)


## Install

1. Install rust with: `curl https://sh.rustup.rs -sSf | sh`. (https://www.rustup.rs/)
2. Install rups with: `cargo install --git https://github.com/nickez/rups`
3. Run with `rups -h` to see the built-in help.

## Example Usage

1. Launch python through rups: `rups python`
2. Connect to python through a separate terminal: `telnet localhost 3000`
