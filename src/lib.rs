#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
extern crate futures;
extern crate libc;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;

use std::mem::PinMut;

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpStream, TcpListener};

use futures::future::{Future, FutureObj};
use futures::stream::Stream;
use futures::io::AsyncRead;
use futures::io::AsyncWrite;
use futures::io::Error;
use futures::task::Context;
use futures::task::{Executor, LocalWaker, SpawnObjError};
use futures::task::{Wake, Waker};
use futures::Poll;
use libc::{fd_set, select, timeval, FD_ISSET, FD_SET, FD_ZERO};

use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;

use std::net::ToSocketAddrs;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

// reactor lives in a thread local variable. Here's where all magic happens!
thread_local! {
    static REACTOR: Rc<EventLoop> = Rc::new(EventLoop::new());
}

pub fn run<F: Future<Output = ()> + Send + 'static>(f: F) {
    REACTOR.with(|reactor| reactor.run(f))
}

// Our waker Token. It stores the index of the future in the wait queue
// (see below)
#[derive(Debug)]
struct Token(usize);

impl Wake for Token {
    fn wake(arc_self: &Arc<Token>) {
        debug!("waking {:?}", arc_self);

        let Token(idx) = **arc_self;

        // get access to the reactor by way of TLS and call wake
        REACTOR.with(|reactor| {
            let wakeup = Wakeup {
                index: idx,
                waker: unsafe { futures::task::local_waker(arc_self.clone()) },
            };
            reactor.wake(wakeup);
        });
    }
}

// Task is a boxed future
struct Task {
    future: FutureObj<'static, ()>,
}

impl Task {
    // returning Ready will lead to task being removed from wait queues and droped
    fn poll<E: Executor>(&mut self, waker: LocalWaker, exec: &mut E) -> Poll<()> {
        let mut context = Context::new(&waker, exec);

        let future = PinMut::new(&mut self.future);

        match future.poll(&mut context) {
            Poll::Ready(_) => {
                debug!("future done");
                Poll::Ready(())
            }
            Poll::Pending => {
                debug!("future not yet ready");
                Poll::Pending
            }
        }
    }
}

// reactor handle, just like in real tokio
pub struct Handle(Rc<EventLoop>);

impl Executor for Handle {
    fn spawn_obj(&mut self, f: FutureObj<'static, ()>) -> Result<(), SpawnObjError> {
        debug!("spawning from handle");
        self.0.do_spawn(f);
        Ok(())
    }
}

struct Wakeup {
    index: usize,
    waker: LocalWaker,
}

// The "real" event loop.
struct EventLoop {
    read: RefCell<BTreeMap<RawFd, Waker>>,
    write: RefCell<BTreeMap<RawFd, Waker>>,
    counter: Cell<usize>,
    wait_queue: RefCell<Vec<Task>>,
    run_queue: RefCell<VecDeque<Wakeup>>,
}

impl EventLoop {
    pub fn new() -> Self {
        EventLoop {
            read: RefCell::new(BTreeMap::new()),
            write: RefCell::new(BTreeMap::new()),
            counter: Cell::new(0),
            wait_queue: RefCell::new(Vec::new()),
            run_queue: RefCell::new(VecDeque::new()),
        }
    }

    pub fn handle(&self) -> Handle {
        REACTOR.with(|reactor| Handle(reactor.clone()))
    }

    // a future calls this to register its interest
    // in socket's "ready to be read" events
    fn add_read_interest(&self, fd: RawFd, waker: Waker) {
        debug!("adding read interest for {}", fd);

        if !self.read.borrow().contains_key(&fd) {
            self.read.borrow_mut().insert(fd, waker);
        }
    }

    fn remove_read_interest(&self, fd: RawFd) {
        debug!("removing read interest for {}", fd);

        self.read.borrow_mut().remove(&fd);
    }

    // see above
    fn remove_write_interest(&self, fd: RawFd) {
        debug!("removing write interest for {}", fd);

        self.write.borrow_mut().remove(&fd);
    }

    fn add_write_interest(&self, fd: RawFd, waker: Waker) {
        debug!("adding write interest for {}", fd);

        if !self.write.borrow().contains_key(&fd) {
            self.write.borrow_mut().insert(fd, waker);
        }
    }

    // waker calls this to put the future on the run queue
    fn wake(&self, wakeup: Wakeup) {
        self.run_queue.borrow_mut().push_back(wakeup);
    }

    fn next_task(&self) -> LocalWaker {
        let counter = self.counter.get();
        let w = Arc::new(Token(counter));
        self.counter.set(counter + 1);
        unsafe { futures::task::local_waker(w) }
    }

    // create a task, poll it once and push it on wait queue
    fn do_spawn<F: Future<Output = ()> + Send + 'static>(&self, f: F) {
        let waker = self.next_task();
        let f = Box::new(f);
        let mut task = Task {
            future: FutureObj::new(f),
        };
        let mut handle = self.handle();

        {
            task.poll(waker, &mut handle)
        };

        self.wait_queue.borrow_mut().push(task)
    }

    // the meat of the event loop
    // we're using select(2) because it's simple and it's portable
    pub fn run<F: Future<Output = ()> + Send + 'static>(&self, f: F) {
        self.do_spawn(f);

        loop {
            debug!("select loop start");

            // event loop iteration timeout. if no descriptor
            // is ready we continue iterating
            let mut tv: timeval = timeval {
                tv_sec: 1,
                tv_usec: 0,
            };

            // initialize fd_sets (file descriptor sets)
            let mut read_fds: fd_set = unsafe { std::mem::zeroed() };
            let mut write_fds: fd_set = unsafe { std::mem::zeroed() };

            unsafe { FD_ZERO(&mut read_fds) };
            unsafe { FD_ZERO(&mut write_fds) };

            let mut nfds = 0;

            // add read interests to read fd_sets
            for fd in self.read.borrow().keys() {
                debug!("added fd {} for read", fd);
                unsafe { FD_SET(*fd, &mut read_fds as *mut fd_set) };
                nfds = std::cmp::max(nfds, fd + 1);
            }

            // add write interests to write fd_sets
            for fd in self.write.borrow().keys() {
                debug!("added fd {} for write", fd);
                unsafe { FD_SET(*fd, &mut write_fds as *mut fd_set) };
                nfds = std::cmp::max(nfds, fd + 1);
            }

            // select will block until some event happens
            // on the fds or timeout triggers
            let rv = unsafe {
                select(
                    nfds,
                    &mut read_fds,
                    &mut write_fds,
                    std::ptr::null_mut(),
                    &mut tv,
                )
            };

            // don't care for errors
            if rv == -1 {
                panic!("select()");
            } else if rv == 0 {
                debug!("timeout");
            } else {
                debug!("data available on {} fds", rv);
            }

            // check which fd it was and put appropriate future on run queue
            for (fd, waker) in self.read.borrow().iter() {
                let is_set = unsafe { FD_ISSET(*fd, &mut read_fds as *mut fd_set) };
                debug!("fd#{} set (read)", fd);
                if is_set {
                    waker.wake();
                }
            }

            // same for write
            for (fd, waker) in self.write.borrow().iter() {
                let is_set = unsafe { FD_ISSET(*fd, &mut write_fds as *mut fd_set) };
                debug!("fd#{} set (write)", fd);
                if is_set {
                    waker.wake();
                }
            }

            let mut tasks_done = Vec::new();

            // now pop futures from the run queue and poll them
            while let Some(w) = self.run_queue.borrow_mut().pop_front() {
                debug!("polling task#{}", w.index);

                let mut handle = self.handle();

                // if a task returned Ready - we're done with it
                if let Some(ref mut task) = self.wait_queue.borrow_mut().get_mut(w.index) {
                    if let Poll::Ready(_) = task.poll(w.waker, &mut handle) {
                        tasks_done.push(w.index);
                    }
                }
            }

            // remove completed tasks
            for idx in tasks_done {
                info!("removing task#{}", idx);
                self.wait_queue.borrow_mut().remove(idx);
            }

            // stop the loop if no more tasks
            if self.wait_queue.borrow().is_empty() {
                return;
            }
        }
    }
}

impl Executor for EventLoop {
    fn spawn_obj(&mut self, f: FutureObj<'static, ()>) -> Result<(), SpawnObjError> {
        self.do_spawn(f);
        Ok(())
    }
}

// AsyncTcpStream just wraps std tcp stream
#[derive(Debug)]
pub struct AsyncTcpStream(TcpStream);

impl AsyncTcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<AsyncTcpStream, std::io::Error> {
        let inner = TcpStream::connect(addr)?;

        inner.set_nonblocking(true)?;
        Ok(AsyncTcpStream(inner))
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
    fn poll_read(&mut self, cx: &mut Context, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
        debug!("poll_read() called");

        let fd = self.0.as_raw_fd();
        let waker = cx.waker();

        match self.0.read(buf) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_read_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }
}

impl AsyncWrite for AsyncTcpStream {
    fn poll_write(&mut self, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize, Error>> {
        debug!("poll_write() called");

        let fd = self.0.as_raw_fd();
        let waker = cx.waker();

        match self.0.write(buf) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_write_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }

    fn poll_flush(&mut self, _cx: &mut Context) -> Poll<Result<(), Error>> {
        debug!("poll_flush() called");
        Poll::Ready(Ok(()))
    }

    fn poll_close(&mut self, _cx: &mut Context) -> Poll<Result<(), Error>> {
        debug!("poll_close() called");
        Poll::Ready(Ok(()))
    }
}

// AsyncTcpListener just wraps std tcp listener
#[derive(Debug)]
pub struct AsyncTcpListener(TcpListener);

impl AsyncTcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<AsyncTcpListener, std::io::Error> {
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

    fn poll_next(self: PinMut<Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        debug!("poll_next() called");

        let fd = self.0.as_raw_fd();
        let waker = cx.waker();

        match self.0.accept() {
            Ok((conn, _)) => {
                conn.set_nonblocking(true).unwrap();
                Poll::Ready(Some(AsyncTcpStream(conn)))
            },
            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                REACTOR.with(|reactor| reactor.add_read_interest(fd, waker.clone()));

                Poll::Pending
            }
            Err(err) => panic!("error {:?}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::AsyncReadExt;
    use futures::io::AsyncWriteExt;
    use futures::FutureExt;
    use futures::StreamExt;
    use futures::TryFutureExt;

    async fn write_and_read(mut stream: AsyncTcpStream, data: String) -> Result<String, std::io::Error> {
        await!(stream.write_all(data.as_bytes()))?;
        println!("written {}", data);
        let mut buf = vec![0; 128];
        let len = await!(stream.read(&mut buf))?;
        println!("read: {}", String::from_utf8_lossy(&buf[0..len]));
        Ok(String::new())
    }

    #[test]
    fn listener() {
        let listener = AsyncTcpListener::bind("127.0.0.1:9000").unwrap();
        println!("listening on 127.0.0.1:9000");
        let server = listener.incoming()
                        .for_each(|stream| {
                            write_and_read(stream, "hello world\n".into()).map_err(|_| ()).map(|_| ())
                        });
        run(server);
    }

    #[test]
    fn client() {
        pretty_env_logger::init();

        let stream = AsyncTcpStream::connect("127.0.0.1:9000").unwrap();
        println!("running");
        let future = write_and_read(stream, "Hello world".to_owned())
            .map_err(|_| ())
            .map(|_| ());
        run(future);
    }
}
