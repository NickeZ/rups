extern crate futures;
extern crate libc;
extern crate mio;
extern crate tokio_core;
#[macro_use] extern crate log;
extern crate futures_addition;

use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::os::unix::io::{RawFd, FromRawFd};
use std::ptr;
use std::process;
use std::fs;

use futures::{Stream, Sink, StartSend, AsyncSink, Async, Poll, Future};
use futures::sync::oneshot;
use mio::{Evented, PollOpt, Ready, Token};
use mio::unix::{EventedFd, UnixReady};
use tokio_core::reactor::{Handle, PollEvented};

use futures_addition::send_all::HasItem;

#[allow(dead_code)]
fn printfds(prefix: &str) {
    for entry in fs::read_dir("/proc/self/fd").unwrap() {
        let entry = entry.unwrap();
        if let Ok(canon_path) = fs::canonicalize(entry.path()) {
            let path = entry.path();
            debug!("{}: {:?} {:?}", prefix, path, canon_path);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    extern crate env_logger;
    use tokio_core::reactor::Core;
    use futures::Future;

    const STIMULI: [u8; 5] = ['h' as u8, 'e' as u8, 'j' as u8, '\n' as u8, '\x04' as u8];

    #[test]
    fn it_works() {
        env_logger::init().unwrap();
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let pty = ::Pty::new();
        let mut child = pty.spawn(process::Command::new("cat"), &handle).unwrap();

        let mut stim = Vec::new();
        for c in STIMULI.iter() {
            stim.push(c.clone());
        }

        let output = child.output().take().unwrap().for_each(|x| {
            print!("OUT {}", String::from_utf8(x).unwrap());
            Ok(())
        });

        let input = child.input().take().unwrap()
            .send(stim)
            .and_then(|f| f.flush());

        if log_enabled!(log::LogLevel::Debug){
            printfds("before core");
        }

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
    master: RawFd,
    slave: Option<RawFd>,
}

impl Pty {
    pub fn new() -> Pty
    {
        let (master, slave) = openpty(24u16, 80u16);
        debug!("ptm: {}, pts: {}", master, slave);
        //unsafe {
        //    // If child dies, reap it
        //    libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        //}

        Pty {
            master: master as RawFd,
            slave: Some(slave as RawFd),
        }
    }

    pub fn spawn(mut self, mut command: process::Command, handle: &Handle) -> io::Result<Child>
    {
        debug!("spawning {:?}", command);
        let (master, slave) = (self.master, self.slave.take().unwrap());
        // Box up the slave interface so that it is moved safely to the child process.
        let child_slave = Box::new(slave);
        // process::Stdio will take care of closing slave. It uses FileDesc::drop internally which
        // ignores errors on libc::close. We can therefore safely create copies of slave.
        let childin = unsafe { process::Stdio::from_raw_fd(slave) };
        let childout = unsafe { process::Stdio::from_raw_fd(slave) };
        let slave = unsafe { process::Stdio::from_raw_fd(slave) };
        command.stdin(childin)
            .stdout(childout)
            .stderr(slave)
            .before_exec(move || {
                // Make this process leader of new session
                let slave = *child_slave;
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

                if log_enabled!(log::LogLevel::Debug) {
                    printfds("child before exec");
                }

                Ok(())
            });

        let child = command.spawn()?;

        let master2 = cvt(unsafe {libc::dup(master)})?;

        if log_enabled!(log::LogLevel::Debug) {
            printfds("parent after fork");
        }

        let (stream_done_tx, stream_done_rx) = oneshot::channel::<i32>();
        let io = stdio(master, handle);
        let output = PtyStream::new(
            io.expect("Failed to create EventedFd for output").unwrap(),
            stream_done_rx,
        );

        let (sink_done_tx, sink_done_rx) = oneshot::channel::<i32>();
        let io = stdio(master2, handle);
        let input = PtySink::new(
            io.expect("Failed to create EventedFd for input").unwrap(),
            sink_done_rx
        );

        let child = Child {
            inner: child,
            master: (master, master2),
            input: Some(input),
            output: Some(output),
            sink_done: Some(sink_done_tx),
            stream_done: Some(stream_done_tx),
        };
        Ok(child)
    }
}

pub struct Child {
    inner: process::Child,
    master: (RawFd, RawFd),
    input: Option<PtySink>,
    output: Option<PtyStream>,
    sink_done: Option<oneshot::Sender<i32>>,
    stream_done: Option<oneshot::Sender<i32>>,
}

impl Child {
    pub fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        println!("wait for child");
        //self.sink_done.take().unwrap().send(1).unwrap();
        //self.stream_done.take().unwrap().send(1).unwrap();
        let sink = self.sink_done.take().unwrap();
        match sink.send(1) {
            Ok(()) => debug!("killing sink"),
            Err(e) => debug!("sink already deallocated {:?}", e),
        }
        let stream = self.stream_done.take().unwrap();
        match stream.send(1) {
            Ok(()) => debug!("killing stream"),
            Err(e) => debug!("stream already deallocated {:?}", e),
        }
        self.inner.wait()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    pub fn set_window_size(&mut self, rows: Rows, columns: Columns) {
        info!("set rows: {:?}, cols: {:?}", rows, columns);
        let mut ws = get_winsize(self.master.0).unwrap();
        let Rows(ws_row) = rows;
        let Columns(ws_col) = columns;
        ws.ws_row = ws_row;
        ws.ws_col = ws_col;
        set_winsize(self.master.0, &ws).expect("Failed to set window size");
    }

    pub fn output(&mut self) -> &mut Option<PtyStream> {
        &mut self.output
    }

    pub fn input(&mut self) -> &mut Option<PtySink> {
        &mut self.input
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        // Ignore error for same reason as FileDesc in rust stdlib.
        debug!("Dropping stdin/stdout fds {} {}", self.master.0, self.master.1);
        let _ = unsafe{ libc::close(self.master.0) };
        let _ = unsafe{ libc::close(self.master.1) };
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

    cvt(unsafe {
        libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null(), &win)
    }).unwrap();

    (master, slave)
}

pub fn get_winsize(fd: RawFd) -> io::Result<libc::winsize> {
    let mut ws = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        cvt(libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws))?;
        Ok((ws))
    }
}

pub fn set_winsize(fd: RawFd, ws: &libc::winsize) -> io::Result<()> {
    unsafe {
        cvt(libc::ioctl(fd, libc::TIOCSWINSZ, ws)).map(|_| ())
    }
}

/*
 *
 * ===== Pipe =====
 *
 */

pub struct PtyStream {
    ptyio: PtyIo,
    done: oneshot::Receiver<i32>,
}

impl PtyStream {
    pub fn new(ptyio: PtyIo, done: oneshot::Receiver<i32>) -> PtyStream {
        PtyStream {
            ptyio: ptyio,
            done: done,
        }
    }
}

impl Stream for PtyStream {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>{
        let mut buf = [0 as u8;2048];
        match self.done.poll() {
            Ok(Async::Ready(t)) => return Err(io::Error::new(io::ErrorKind::Other, "died")),
            _ => (),
        }
        match self.ptyio.read(&mut buf) {
            Ok(len) => {
                trace!("Read {} bytes from {:?}", len, self.ptyio);
                let mut vec = Vec::new();
                for i in 0..len {
                    vec.push(buf[i]);
                }
                //println!("{:?}", vec);
                return Ok(Async::Ready(Some(vec)));
            },
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    return Ok(Async::NotReady);
                }
                if e.kind() == io::ErrorKind::Other {
                    // Process probably exited
                    warn!("Failed to read from {:?}: {:?}, process died?", self.ptyio, e);
                    return Err(e);
                    //return Ok(Async::Ready(None));
                }
                warn!("Failed to read: {:?}", e);
                return Err(e);
            },
        }
    }
}

#[derive(Debug)]
pub enum PtySinkError<T> {
    IoError(io::Error),
    TryAgain(T),
}

impl<T> From<io::Error> for PtySinkError<T> {
    fn from(error: io::Error) -> Self {
        PtySinkError::IoError(error)
    }
}

impl<T> HasItem<T> for PtySinkError<T> {
    fn item(self) -> Option<T> {
        match self {
            PtySinkError::TryAgain(item) => Some(item),
            _ => None,
        }
    }
}

pub struct PtySink {
    pub ptyin: PtyIo,
    buf: Vec<u8>,
    done: oneshot::Receiver<i32>,
}

impl PtySink {
    pub fn new(ptyin: PtyIo, done: oneshot::Receiver<i32>) -> PtySink {
        PtySink {
            ptyin: ptyin,
            buf: Vec::new(),
            done: done,
        }
    }
}

impl Sink for PtySink {
    type SinkItem = Vec<u8>;
    type SinkError = PtySinkError<Self::SinkItem>;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let orig_len = item.len();
        let written_len;
        if orig_len == 0 {
            return Ok(AsyncSink::Ready);
        }
        match self.done.poll() {
            Ok(Async::Ready(t)) => return Err(PtySinkError::TryAgain(item)),
            _ => (),
        }
        {
            match self.ptyin.write(item.as_slice()) {
                Ok(len) => {
                    trace!("wrote {} to {:?}", len, self.ptyin);
                    std::io::Write::flush(self.ptyin.get_mut()).expect("failed to flush");
                    written_len = len;
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        trace!("not ready to write: {:?}", e);
                        return Ok(AsyncSink::NotReady(item));
                    }
                    warn!("start_send(): Failed to write: {:?}", e);
                    //return Err(From::from(e));
                    return Err(PtySinkError::TryAgain(item));
                }
            }
        }
        self.buf.clear();
        if written_len != orig_len {
            for x in item[written_len..orig_len-1].iter() {
                self.buf.push(*x);
            }
        }
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
            trace!("PC Will write to {:?}", self.ptyin);
            match self.ptyin.write(self.buf.as_slice()) {
                Ok(len) => {
                    trace!("wrote {}", len);
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
                        warn!("poll_complete(): Failed to write: {:?}", e);
                        //res = Ok(Async::Ready(None));
                        res = Err(From::from(e));
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

#[derive(Debug)]
pub struct Fd(RawFd);

impl io::Read for Fd {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let len = cvt(unsafe {
            libc::read(self.0,
                       bytes.as_ptr() as *mut libc::c_void,
                       bytes.len())
        })?;
        Ok(len as usize)
    }
}

impl io::Write for Fd {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let len = cvt(unsafe {
            libc::write(self.0,
                        bytes.as_ptr() as *mut libc::c_void,
                        bytes.len())
        })?;
        Ok(len as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Evented for Fd {
    fn register(&self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt)
                -> io::Result<()> {
        EventedFd(&self.0).register(poll,
                                    token,
                                    interest | UnixReady::hup(),
                                    opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt)
                  -> io::Result<()> {
        EventedFd(&self.0).reregister(poll,
                                      token,
                                      interest | UnixReady::hup(),
                                      opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0).deregister(poll)
    }
}

type PtyIo = PollEvented<Fd>;

// The Fd in T has to be unique so that tokio can keep track of everything..
fn stdio(fd: RawFd, handle: &Handle)
            -> io::Result<Option<PollEvented<Fd>>>
{
    debug!("Creating PollEvented for fd {:?}", fd);
    // Set the fd to nonblocking before we pass it to the event loop
    set_nonblock(fd)?;
    let io = PollEvented::new(Fd(fd), handle)?;
    Ok(Some(io))
}
