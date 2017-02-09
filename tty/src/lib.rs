extern crate futures;
extern crate libc;
extern crate mio;
extern crate tokio_core;

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::os::unix::io::{RawFd, IntoRawFd, AsRawFd, FromRawFd};
use std::ptr;
use std::process;

//use libc::{winsize, c_int, pid_t, WNOHANG, WIFEXITED, SIGCHLD, TIOCSCTTY, O_NONBLOCK, F_SETFL, F_GETFL};
use libc::{winsize, TIOCSCTTY, O_NONBLOCK, F_SETFL, F_GETFL};
use futures::{Future, Stream, Sink, StartSend, AsyncSink, Async, Poll};


#[cfg(test)]
mod tests {
    use futures::{Future, Stream, Sink};
    use tokio_core::reactor;
    #[test]
    fn it_works() {
        let mut builder = ::Command::new("python");
        //builder.arg("Cargo.toml");
        let child = builder.spawn().unwrap();
        println!("TEST");

        let mut core = reactor::Core::new().unwrap();

        let output = child.output().for_each(|x| {
            print!("{}", String::from_utf8(x).unwrap());
            //println!("{:?}", x);
            Ok(())
        });

        let input = child.input()
            .send(vec!['h' as u8, 'e' as u8, 'j' as u8, '\n' as u8, '\x04' as u8])
            .and_then(|f| f.flush());
            //.map(|_| child.input_close());
            //.and_then(|f| {child.input_close(); Box::new(Future::new<PipeWriter::Item, std::io::Error>()) });

        core.run(input.join(output)).unwrap();
    }
}

pub struct Rows(u16);
pub struct Columns(u16);

pub struct Command {
    builder: process::Command,
    master: libc::c_int,
    slave: libc::c_int,
}

impl Command {
    pub fn new<S>(program: S) -> Command
        where S: AsRef<OsStr>
    {
        let (master, slave) = openpty(24 as u8, 80 as u8);

        let mut builder = process::Command::new(program);
        builder.stdin(unsafe { process::Stdio::from_raw_fd(slave) })
               .stdout(unsafe { process::Stdio::from_raw_fd(slave) })
               .stderr(unsafe { process::Stdio::from_raw_fd(slave) })
               .before_exec(move || {
                   // Make this process leader of new session
                   set_sessionid()?;
                   // Set the controlling terminal to the slave side of the pty
                   set_controlling_terminal(slave)?;
                   // Slave and master are not needed anymore
                   unsafe {
                       cvt(libc::close(slave))?;
                       cvt(libc::close(master))?;
                   }
                   // Reset all signal handlers to default
                   unsafe {
                       libc::signal(libc::SIGCHLD, libc::SIG_DFL);
                       libc::signal(libc::SIGHUP, libc::SIG_DFL);
                       libc::signal(libc::SIGINT, libc::SIG_DFL);
                       libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                       libc::signal(libc::SIGTERM, libc::SIG_DFL);
                       libc::signal(libc::SIGALRM, libc::SIG_DFL);
                   }
                   Ok(())
               });
        Command {
            builder: builder,
            master: master,
            slave: slave,
        }
    }

    pub fn arg<S>(&mut self, arg: S) -> &mut Command
        where S: AsRef<OsStr>
    {
        self.builder.arg(arg);
        self
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        set_nonblock(self.master)?;
        let child = self.builder.spawn()?;
        unsafe {
            // Slave side of pty is not needed anymore
            libc::close(self.slave);
            // If child dies, reap it
            libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        }
        Ok(Child::new(child, self.master))
    }

    pub fn set_window_size(rows: Rows, columns: Columns) {
        //let mut ws = get_winsize(stdin).unwrap()
        //ws.ws_row = Rows;
        //ws.ws_col = Columns;
        //set_winsize(stdin, &ws);
    }
}

pub struct Child {
    child: process::Child,
    master: libc::c_int,
}

impl Child {
    pub fn new(child: process::Child, master: libc::c_int) -> Child {
        Child {
            child: child,
            master: master,
        }
    }

    pub fn output(&self) -> PipeReader {
        unsafe {PipeReader::from_raw_fd(self.master) }
    }

    pub fn input(&self) -> PipeWriter {
        unsafe {PipeWriter::from_raw_fd(self.master) }
    }

    pub fn input_close(&self) {
        unsafe {libc::close(self.master) };
    }
}

/// Get raw fds for master/slave ends of a new pty
#[cfg(target_os = "linux")]
fn openpty(rows: u8, cols: u8) -> (RawFd, RawFd) {
    let mut master: RawFd = 0;
    let mut slave: RawFd = 0;

    let win = winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: cols as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe {
        libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null(), &win)
    };

    if res < 0 {
        println!("openpty failed");
    }

    (master, slave)
}


/*
 *
 * ===== Pipe =====
 *
 */

#[derive(Debug)]
pub struct PipeReader {
    io: Io,
}

impl PipeReader {
    pub fn from_stdout(stdout: process::ChildStdout) -> io::Result<Self> {
        match set_nonblock(stdout.as_raw_fd()) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeReader::from(unsafe { Io::from_raw_fd(stdout.into_raw_fd()) }));
    }
    pub fn from_stderr(stderr: process::ChildStderr) -> io::Result<Self> {
        match set_nonblock(stderr.as_raw_fd()) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeReader::from(unsafe { Io::from_raw_fd(stderr.into_raw_fd()) }));
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl<'a> Read for &'a PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.io).read(buf)
    }
}

impl Stream for PipeReader {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>{
        let mut buf = [0 as u8;2048];
        loop {
            match self.read(&mut buf) {
                Ok(len) => {
                    let mut vec = Vec::new();
                    for i in 0..len {
                        vec.push(buf[i]);
                    }
                    //println!("{:?}", vec);
                    return Ok(Async::Ready(Some(vec)));
                },
                Err(e) => {
                    //println!("{:?}", e);
                    if e.kind() == io::ErrorKind::WouldBlock {
                        // unpark to make reactor poll this stream again
                        ::futures::task::park().unpark();
                        return Ok(Async::NotReady);
                    }
                    return Err(e);
                },
            }
        }
    }
}

impl From<Io> for PipeReader {
    fn from(io: Io) -> PipeReader {
        PipeReader { io: io }
    }
}

#[derive(Debug)]
pub struct PipeWriter {
    io: Io,
    buf: Vec<u8>,
}

impl PipeWriter {
    pub fn from_stdin(stdin: process::ChildStdin) -> io::Result<Self> {
        match set_nonblock(stdin.as_raw_fd()) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeWriter::from(unsafe { Io::from_raw_fd(stdin.into_raw_fd()) }));
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl<'a> Write for &'a PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.io).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.io).flush()
    }
}

impl Sink for PipeWriter {
    type SinkItem = Vec<u8>;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.buf.clear();
        for x in item {
            self.buf.push(x);
        }
        //println!("{:?}", self.buf);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        if self.buf.len() == 0 {
            return Ok(Async::Ready(()));
        }
        let mut copy:Vec<u8> = self.buf.drain(..).collect();
        //println!("{:?}", copy);
        match self.write(copy.as_slice()) {
            Ok(len) => {
                //println!("wrote {} bytes", len);
                std::io::Write::flush(self);
                return Ok(Async::Ready(()))
            },
            Err(e) => {
                //self.buf.append(copy.drain(..).collect().iter());
                if e.kind() == io::ErrorKind::WouldBlock {
                    return Ok(Async::NotReady);
                }
                return Err(e);
            },
        }
    }
}

impl From<Io> for PipeWriter {
    fn from(io: Io) -> PipeWriter {
        PipeWriter { io: io, buf: Vec::new() }
    }
}

/*
 *
 * ===== Conversions =====
 *
 */

impl IntoRawFd for PipeReader {
    fn into_raw_fd(self) -> RawFd {
        self.io.into_raw_fd()
    }
}

impl AsRawFd for PipeReader {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromRawFd for PipeReader {
    unsafe fn from_raw_fd(fd: RawFd) -> PipeReader {
        PipeReader { io: FromRawFd::from_raw_fd(fd) }
    }
}

impl IntoRawFd for PipeWriter {
    fn into_raw_fd(self) -> RawFd {
        self.io.into_raw_fd()
    }
}

impl AsRawFd for PipeWriter {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromRawFd for PipeWriter {
    unsafe fn from_raw_fd(fd: RawFd) -> PipeWriter {
        PipeWriter { io: FromRawFd::from_raw_fd(fd), buf: Vec::new() }
    }
}

pub fn set_sessionid() -> io::Result<()> {
    unsafe {
        cvt(libc::setsid()).map(|_| ())
    }
}

pub fn set_controlling_terminal(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        cvt(libc::ioctl(fd, TIOCSCTTY as _, 0)).map(|_| ())
    }
}


pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, F_GETFL, 0);
        cvt(libc::fcntl(fd, F_SETFL, flags | O_NONBLOCK)).map(|_| ())
    }
}

pub fn set_cloexec(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        cvt(libc::ioctl(fd, libc::FIOCLEX)).map(|_| ())
    }
}

/*
 *
 * ===== Basic IO type =====
 *
 */

#[derive(Debug)]
pub struct Io {
    fd: File,
}

impl Io {
    pub fn try_clone(&self) -> io::Result<Io> {
        Ok(Io { fd: try!(self.fd.try_clone()) })
    }
}

impl FromRawFd for Io {
    unsafe fn from_raw_fd(fd: RawFd) -> Io {
        Io { fd: File::from_raw_fd(fd) }
    }
}

impl IntoRawFd for Io {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

impl AsRawFd for Io {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Read for Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.fd).read(dst)
    }
}

impl<'a> Read for &'a Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.fd).read(dst)
    }
}

impl Write for Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.fd).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.fd).flush()
    }
}

impl<'a> Write for &'a Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.fd).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.fd).flush()
    }
}

trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

impl IsMinusOne for i32 {
    fn is_minus_one(&self) -> bool { *self == -1 }
}
impl IsMinusOne for isize {
    fn is_minus_one(&self) -> bool { *self == -1 }
}

fn cvt<T: IsMinusOne>(t: T) -> ::io::Result<T> {
    use std::io;

    if t.is_minus_one() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}
