use fahrenheit::AsyncTcpStream;
use futures::io::{AsyncReadExt, AsyncWriteExt};

async fn http_get(addr: &str) -> Result<String, std::io::Error> {
    let mut conn = AsyncTcpStream::connect(addr)?;
    let _ = conn.write_all(b"GET / HTTP/1.0\r\n\r\n").await?;
    let mut page = Vec::new();
    loop {
        let mut buf = vec![0; 128];
        let len = conn.read(&mut buf).await?;
        if len == 0 {
            break;
        }
        page.extend_from_slice(&buf[..len]);
    }
    let page = String::from_utf8_lossy(&page).into();
    Ok(page)
}

async fn get_google() {
    let res = http_get("google.com:80").await.unwrap();
    println!("{}", res);
}

fn main() {
    fahrenheit::run(get_google())
}
