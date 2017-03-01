extern crate futures;
extern crate libc;
extern crate mio;
extern crate tokio_core;

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::os::unix::io::{RawFd, AsRawFd, FromRawFd};
use std::ptr;
use std::process;
use std::rc::Rc;
use std::cell::RefCell;

use futures::{Stream, Sink, StartSend, AsyncSink, Async, Poll};
use mio::{Evented, PollOpt, Ready, Token};
use mio::unix::EventedFd;
use tokio_core::reactor::{Handle, PollEvented};


#[cfg(test)]
mod tests {
    use futures::{Future, Stream, Sink};
    use tokio_core::reactor;
    use std::process;

    const STIMULI: [u8; 4] = ['h' as u8, 'e' as u8, 'j' as u8, '\n' as u8];

    #[test]
    fn it_works() {
        let mut core = reactor::Core::new().unwrap();
        let handle = core.handle();

        let mut pty = ::Pty::new(&handle);
        pty.spawn(process::Command::new("cat")).unwrap();

        let mut stim = Vec::new();
        for c in STIMULI.iter() {
            stim.push(c.clone());
        }

        let output = pty.output().take().unwrap().for_each(|x| {
            print!("OUT {}", String::from_utf8(x).unwrap());
            //println!("{:?}", x);
            Ok(())
        });

        let input = pty.input().take().unwrap()
            .send(stim)
            .and_then(|f| f.flush());
            //.map(|_| child.input_close());
            //.and_then(|f| {child.input_close(); Box::new(Future::new<PtySink::Item, std::io::Error>()) });

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

enum PtyInner {
    Child(process::Child),
    ExitStatus(process::ExitStatus),
    NeverSpawned,
}

pub struct Pty {
    child: PtyInner,
    master: RawFd,
    slave: RawFd,
    //input: Option<PtySink>,
    //output: Option<PtyStream>,
}

impl Pty {
    pub fn new(handle: &Handle) -> Pty
    {
        let (master, slave) = openpty(24u16, 80u16);
        //set_nonblock(master).unwrap();
        unsafe {
            // If child dies, reap it
            //libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        }
        //let io = stdio(Some(unsafe{ File::from_raw_fd(libc::dup(master))}), handle);
        //let output = PtyStream::new(io.unwrap().unwrap());

        //let io = stdio(Some(unsafe{ File::from_raw_fd(master)}), handle);
        //let input = PtySink::new(io.expect("Error creating io").expect("Got None instead of Some"));

        Pty {
            child: PtyInner::NeverSpawned,
            master: master as RawFd,
            slave: slave as RawFd,
            //input: Some(input),
            //output: Some(output),
        }
    }

    pub fn spawn(&mut self, mut command: process::Command) -> io::Result<()>
    {
        match self.child {
            PtyInner::Child(_) => return Err(io::Error::new(io::ErrorKind::Other, "Child is already spawned")),
            _ => {
                println!("trying to launch");
                let (master, slave) = (self.master, self.slave);
                let slave = unsafe { libc::dup(slave) };
                //let (master, slave) = openpty(24u16, 80u16);
                //self.master = master;
                //self.slave = slave;
                // Give the child process a duplicated fd because it will close the stdin fd when
                // dying...
                //let slave_dup = unsafe { libc::dup(slave) };
                command.stdin(unsafe { process::Stdio::from_raw_fd(slave) })
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
                            //cvt(libc::close(slave_dup))?;
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
                let child = command.spawn().expect("Faily failed");
                self.child = PtyInner::Child(child);
                Ok(())
            }
        }
    }

    pub fn wait(&mut self) {
        let res;
        match self.child {
            PtyInner::Child(ref mut child) => {
                let status = child.wait().expect("Wait failed");
                res = PtyInner::ExitStatus(status);
                //res = PtyInner::NeverSpawned;
            },
            _ => res = PtyInner::NeverSpawned,
        }
        self.child = res;
    }

    pub fn set_window_size(&mut self, rows: Rows, columns: Columns) {
        println!("set {:?} {:?}", rows, columns);
        let mut ws = get_winsize(self.master).unwrap();
        let Rows(ws_row) = rows;
        let Columns(ws_col) = columns;
        ws.ws_row = ws_row;
        ws.ws_col = ws_col;
        set_winsize(self.master, &ws).expect("Failed to set window size");
    }

    //pub fn output(&mut self) -> &mut Option<PtyStream> {
    //    &mut self.output
    //}

    //pub fn input(&mut self) -> &mut Option<PtySink> {
    //    &mut self.input
    //}
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
    pty: Rc<RefCell<Pty>>,
}

impl PtyStream {
    pub fn new(pty: Rc<RefCell<Pty>>) -> PtyStream {
        PtyStream {
            pty: pty,
        }
    }
}

impl Stream for PtyStream {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>{
        let mut buf = [0 as u8;2048];
        match self.pty.borrow_mut().read(&mut buf) {
            Ok(len) => {
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
                println!("{:?}", e);
                return Err(e);
            },
        }
    }
}

pub struct PtySink {
    pub ptyin: PtyIo,
    buf: Vec<u8>,
}

impl PtySink {
    pub fn new(ptyin: PtyIo) -> PtySink {
        PtySink {
            ptyin: ptyin,
            buf: Vec::new(),
        }
    }
}

impl Sink for PtySink {
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
                    println!("{:?}", e);
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
                        println!("{:?}", e);
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
type PtyIo = PollEvented<Fd<File>>;

// The Fd in T has to be unique so that tokio can keep track of everything..
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
    let io = PollEvented::new(Fd(io), handle)?;
    Ok(Some(io))
}
