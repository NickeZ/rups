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
//use libc::{winsize, TIOCSCTTY, TIOCGWINSZ, TIOCSWINSZ, O_NONBLOCK, F_SETFL, F_GETFL};
use futures::{Stream, Sink, StartSend, AsyncSink, Async, Poll};
use mio::{Evented, PollOpt, Ready, Token};
use mio::unix::EventedFd;
use tokio_core::reactor::{Handle, PollEvented};


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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rows(libc::c_ushort);
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Columns(libc::c_ushort);

impl From<u16> for Rows {
    fn from(val: u16) -> Rows {
        Rows(val)
    }
}

impl From<u16> for Columns {
    fn from(val: u16) -> Columns {
        Columns(val)
    }
}

pub struct Pty {
    builder: process::Command,
    io: Io,
    input: Option<PipeWriter>,
    output: Option<PipeReader>,
}

impl Pty {
    pub fn new<S>(program: S, handle: &Handle) -> Pty
        where S: AsRef<OsStr>
    {
        let (master, slave) = openpty(24u16, 80u16);
        set_nonblock(master).unwrap();

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
        unsafe {
            // If child dies, reap it
            libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        }
        let io = stdio(Some(unsafe{ Io::from_raw_fd(libc::dup(master))}), handle);
        let output = PipeReader::new(io.unwrap().unwrap());

        let io = stdio(Some(unsafe{ Io::from_raw_fd(master)}), handle);
        let input = PipeWriter::new(io.expect("Error creating io").expect("Got None instead of Some"));

        Pty {
            builder: builder,
            io: unsafe {Io::from_raw_fd(master)},
            input: Some(input),
            output: Some(output),
        }
    }

    pub fn arg<S>(&mut self, arg: S) -> &mut Pty
        where S: AsRef<OsStr>
    {
        self.builder.arg(arg);
        self
    }

    pub fn spawn(&mut self) -> io::Result<process::Child> {
        self.builder.spawn()
    }

    pub fn set_window_size(&mut self, rows: Rows, columns: Columns) {
        println!("set {:?} {:?}", rows, columns);
        let mut ws = get_winsize(&self.io).unwrap();
        let Rows(ws_row) = rows;
        let Columns(ws_col) = columns;
        ws.ws_row = ws_row;
        ws.ws_col = ws_col;
        set_winsize(&self.io, &ws).expect("Failed to set window size");
    }

    pub fn output(&mut self) -> &mut Option<PipeReader> {
        &mut self.output
    }

    pub fn input(&mut self) -> &mut Option<PipeWriter> {
        &mut self.input
    }
}

/// Get raw fds for master/slave ends of a new pty
#[cfg(target_os = "linux")]
fn openpty(rows: u16, cols: u16) -> (RawFd, RawFd) {
    let mut master: RawFd = 0;
    let mut slave: RawFd = 0;

    let win = libc::winsize {
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

pub fn get_winsize<T>(io: &T) -> io::Result<libc::winsize> where T: AsRawFd {
    let mut ws = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    match unsafe { libc::ioctl(io.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) } {
        0 => Ok(ws),
        _ => Err(io::Error::last_os_error()),
    }
}

pub fn set_winsize<T>(io: &T, ws: &libc::winsize) -> io::Result<()> where T: AsRawFd {
    match unsafe { libc::ioctl(io.as_raw_fd(), libc::TIOCSWINSZ, ws) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

/*
 *
 * ===== Pipe =====
 *
 */

pub struct PipeReader {
    ptyout: PtyOut,
}

impl PipeReader {
    pub fn new(ptyout: PtyOut) -> PipeReader {
        PipeReader {
            ptyout: ptyout,
        }
    }
}

impl Stream for PipeReader {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>{
        let mut buf = [0 as u8;2048];
        match self.ptyout.read(&mut buf) {
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
                    return Ok(Async::NotReady);
                }
                return Err(e);
            },
        }
    }
}

pub struct PipeWriter {
    pub ptyin: PtyIn,
    buf: Vec<u8>,
}

impl PipeWriter {
    pub fn new(ptyin: PtyIn) -> PipeWriter {
        PipeWriter {
            ptyin: ptyin,
            buf: Vec::new(),
        }
    }
}

impl Sink for PipeWriter {
    type SinkItem = Vec<u8>;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let orig_len = item.len();
        let written_len;
        if orig_len == 0 {
            return Ok(AsyncSink::Ready);
        }
        {
            match self.ptyin.write(item.as_slice()) {
                Ok(len) => {
                    std::io::Write::flush(self.ptyin.get_mut()).expect("failed to flush");
                    written_len = len;
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        return Ok(AsyncSink::NotReady(item));
                    }
                    return Err(e);
                }
            }
        }
        self.buf.clear();
        if written_len != orig_len {
            for x in item[written_len..orig_len-1].iter() {
                self.buf.push(*x);
            }
        }
        println!("{:?}", self.buf);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        let orig_len = self.buf.len();
        let mut res = Ok(Async::NotReady);
        if orig_len == 0 {
            return Ok(Async::Ready(()));
        }
        let mut written_len = 0;
        {
            match self.ptyin.write(self.buf.as_slice()) {
                Ok(len) => {
                    written_len = len;
                    //println!("wrote {} bytes", len);
                    std::io::Write::flush(self.ptyin.get_mut()).expect("failed to flush");
                    if len == orig_len {
                        res = Ok(Async::Ready(()));
                    }
                },
                Err(e) => {
                    //self.buf.append(copy.drain(..).collect().iter());
                    if e.kind() != io::ErrorKind::WouldBlock {
                        res = Err(e);
                    }
                },
            }
        }
        self.buf.drain(0..written_len);
        res
    }
}

pub fn set_sessionid() -> io::Result<()> {
    unsafe {
        cvt(libc::setsid()).map(|_| ())
    }
}

pub fn set_controlling_terminal(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        cvt(libc::ioctl(fd, libc::TIOCSCTTY as _, 0)).map(|_| ())
    }
}

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        cvt(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK)).map(|_| ())
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

pub struct Fd<T>(T);

impl<T: io::Read> io::Read for Fd<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}

impl<T: io::Write> io::Write for Fd<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<T> Evented for Fd<T> where T: AsRawFd {
    fn register(&self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt)
                -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).register(poll,
                                                token,
                                                interest | Ready::hup(),
                                                opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt)
                  -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).reregister(poll,
                                                  token,
                                                  interest | Ready::hup(),
                                                  opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}
type PtyIn = PollEvented<Fd<Io>>;
type PtyOut = PollEvented<Fd<Io>>;

fn stdio<T>(option: Option<T>, handle: &Handle)
            -> io::Result<Option<PollEvented<Fd<T>>>>
    where T: AsRawFd
{
    let io = match option {
        Some(io) => io,
        None => return Ok(None),
    };

    // Set the fd to nonblocking before we pass it to the event loop
    set_nonblock(io.as_raw_fd())?;
    let io = try!(PollEvented::new(Fd(io), handle));
    Ok(Some(io))
}
