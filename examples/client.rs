#![feature(futures_api, async_await, await_macro)]
extern crate fahrenheit;
extern crate futures;

use fahrenheit::AsyncTcpStream;
use futures::io::{AsyncReadExt, AsyncWriteExt};

async fn http_get(addr: &str) -> Result<String, std::io::Error> {
    let mut conn = AsyncTcpStream::connect(addr)?;
    let _ = await!(conn.write_all(b"GET / HTTP/1.0\r\n\r\n"))?;
    let mut page = Vec::new();
    loop {
        let mut buf = vec![0; 128];
        let len = await!(conn.read(&mut buf))?;
        if len == 0 {
            break;
        }
        page.extend_from_slice(&buf[..len]);
    }
    let page = String::from_utf8_lossy(&page).into();
    Ok(page)
}

async fn get_google() {
    let res = await!(http_get("google.com:80")).unwrap();
    println!("{}", res);
}

fn main() {
    fahrenheit::run(get_google())
}
