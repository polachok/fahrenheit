extern crate futures;
extern crate libc;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::mem::zeroed;
use libc::{fd_set, FD_ZERO, FD_ISSET, FD_SET, FD_CLR};
use futures::{Future, FutureExt};
use futures::executor::Executor;
use futures::io::AsyncWrite;
use futures::io::AsyncWriteExt;
use futures::io::AsyncRead;
use futures::io::AsyncReadExt;
use futures::task::Context;
use futures::Async;
use futures::io::Error;

use std::net::ToSocketAddrs;

/* later 
struct EventLoop {
	read: fd_set,
	write: fd_set,
}

impl EventLoop {
	pub fn new() -> Self {
		EventLoop {
			read: unsafe { std::mem::zeroed() },
			write: unsafe { std::mem::zeroed() },
		}
	}

	pub fn run<F: Future<Item = (), Error = ()>>(f: F) {
	}
}
*/

#[derive(Debug)]
struct AsyncTcpStream(TcpStream);

impl AsyncTcpStream {
	pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<AsyncTcpStream, std::io::Error> {
		let inner = TcpStream::connect(addr)?;
		inner.set_nonblocking(true)?;
		Ok(AsyncTcpStream(inner))
	}
}

impl AsyncRead for AsyncTcpStream {
	fn poll_read(&mut self, cx: &mut Context, buf: &mut [u8]) -> Result<Async<usize>, Error> {
		println!("poll_read() called");
		match self.0.read(buf) {
			Ok(len) => Ok(Async::Ready(len)),
			Err(err) => unimplemented!(),
		}
	}
}

impl AsyncWrite for AsyncTcpStream {
	fn poll_write(
	    &mut self, 
	    cx: &mut Context, 
	    buf: &[u8]
	) -> Result<Async<usize>, Error> {
		println!("poll_write() called");
		match self.0.write(buf) {
			Ok(len) => Ok(Async::Ready(len)),
			Err(err) => unimplemented!(),
		}
	}

	fn poll_flush(&mut self, cx: &mut Context) -> Result<Async<()>, Error> {
		println!("poll_flush() called");
		Ok(Async::Ready(()))
	}	

	fn poll_close(&mut self, cx: &mut Context) -> Result<Async<()>, Error> {
		println!("poll_close() called");
		Ok(Async::Ready(()))
	}	
}

#[cfg(test)]
mod tests {
	use super::*;

    #[test]
    fn it_works() {
    	use futures::executor::LocalPool;
		let stream = AsyncTcpStream::connect("127.0.0.1:9000").unwrap();
		let pool = LocalPool::new();
		let mut exec = pool.executor();
		println!("running");
		let data: Vec<u8> = (0..1000).map(|_| b'a').collect();
		exec.spawn_local(stream.write_all(data).then(|_| Ok(()))).unwrap();
    }
}
