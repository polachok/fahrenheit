#![feature(futures_api,async_await,await_macro)]
extern crate futures;
extern crate toykio;

use futures::io::{AsyncWriteExt,AsyncReadExt};
use toykio::AsyncTcpStream;

async fn http_get(addr: &str) -> Result<String, std::io::Error> {
	let mut conn = AsyncTcpStream::connect(addr)?;
	let _ = await!(conn.write_all(b"GET / HTTP/1.0\r\n\r\n"))?;
	let mut buf = vec![0;1024];
	let len = await!(conn.read(&mut buf))?;
	let res = String::from_utf8_lossy(&buf[..len]).to_string();
	Ok(res)
}

async fn get_google() {
	let res = await!(http_get("google.com:80")).unwrap();
	println!("{}", res);
}

fn main() {
	toykio::run(get_google())
}