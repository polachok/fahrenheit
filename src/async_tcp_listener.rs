use std::pin::Pin;
use std::io;
use std::net::TcpListener;
use std::net::ToSocketAddrs;
use std::os::unix::io::AsRawFd;
use std::task::Context;

use futures::Poll;
use futures::Stream;

use crate::AsyncTcpStream;
use crate::REACTOR;

use log::debug;

// AsyncTcpListener just wraps std tcp listener
#[derive(Debug)]
pub struct AsyncTcpListener(TcpListener);

impl AsyncTcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<AsyncTcpListener, io::Error> {
        let inner = TcpListener::bind(addr)?;

        inner.set_nonblocking(true)?;
        Ok(AsyncTcpListener(inner))
    }

    pub fn incoming(self) -> Incoming {
        Incoming(self.0)
    }
}

pub struct Incoming(TcpListener);

impl Stream for Incoming {
    type Item = AsyncTcpStream;

    fn poll_next(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Option<Self::Item>> {
        debug!("poll_next() called");

        let fd = self.0.as_raw_fd();
        let waker = ctx.waker();

        match self.0.accept() {
            Ok((conn, _)) => {
                let stream = AsyncTcpStream::from_std(conn).unwrap();
                Poll::Ready(Some(stream))
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_read_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }
}
