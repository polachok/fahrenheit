extern crate futures;
extern crate libc;

use std::io::Write;
use std::io::TcpStream;
use std::mem::zeroed;
use libc::{fd_set, FD_ZERO, FD_ISSET, FD_SET, FD_CLR};
use futures::Future;
use futures::executor::Executor;

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

pub fn write_all<W: Write>(w: W, buf: &[u8]) -> WriteAll {
}

struct WriteAll {
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
