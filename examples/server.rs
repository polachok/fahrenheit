#![feature(futures_api,async_await,await_macro)]
extern crate futures;
extern crate fahrenheit;

use futures::io::AsyncReadExt;
use futures::stream::StreamExt;
use fahrenheit::AsyncTcpStream;
use fahrenheit::AsyncTcpListener;
use std::net::SocketAddr;

async fn listen(addr: &str) {
    let addr: SocketAddr = addr.parse().unwrap();
    let listener = AsyncTcpListener::bind(addr).unwrap();
    let mut incoming = listener.incoming();

    while let Some(stream) = await!(incoming.next()) {
        fahrenheit::spawn(process(stream));
    }
}

async fn process(mut stream: AsyncTcpStream) {
    let mut buf = vec![0;10];
    await!(stream.read_exact(&mut buf));
    println!("{}", String::from_utf8_lossy(&buf));
}

fn main() {
    fahrenheit::run(listen("127.0.0.1:12345"))
}
