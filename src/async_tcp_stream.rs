use std::net::ToSocketAddrs;
use std::net::TcpStream;
use std::os::unix::io::AsRawFd;
use std::io::{self, Read, Write};

use futures::io::AsyncRead;
use futures::io::AsyncWrite;
use futures::io::Error;
use futures::task::LocalWaker;
use futures::Poll;

use crate::REACTOR;

// AsyncTcpStream just wraps std tcp stream
#[derive(Debug)]
pub struct AsyncTcpStream(TcpStream);

impl AsyncTcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<AsyncTcpStream, io::Error> {
        let inner = TcpStream::connect(addr)?;

        inner.set_nonblocking(true)?;
        Ok(AsyncTcpStream(inner))
    }

    pub fn from_std(stream: TcpStream) -> Result<AsyncTcpStream, io::Error> {
    	stream.set_nonblocking(true)?;
    	Ok(AsyncTcpStream(stream))
    }
}

impl Drop for AsyncTcpStream {
    fn drop(&mut self) {
        REACTOR.with(|reactor| {
            let fd = self.0.as_raw_fd();
            reactor.remove_read_interest(fd);
            reactor.remove_write_interest(fd);
        });
    }
}

impl AsyncRead for AsyncTcpStream {
    fn poll_read(&mut self, waker: &LocalWaker, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
        debug!("poll_read() called");

        let fd = self.0.as_raw_fd();

        match self.0.read(buf) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_read_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }
}

impl AsyncWrite for AsyncTcpStream {
    fn poll_write(&mut self, waker: &LocalWaker, buf: &[u8]) -> Poll<Result<usize, Error>> {
        debug!("poll_write() called");

        let fd = self.0.as_raw_fd();

        match self.0.write(buf) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_write_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }

    fn poll_flush(&mut self, _lw: &LocalWaker) -> Poll<Result<(), Error>> {
        debug!("poll_flush() called");
        Poll::Ready(Ok(()))
    }

    fn poll_close(&mut self, _lw: &LocalWaker) -> Poll<Result<(), Error>> {
        debug!("poll_close() called");
        Poll::Ready(Ok(()))
    }
}
