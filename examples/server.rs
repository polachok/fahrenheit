use std::net::SocketAddr;

use fahrenheit::AsyncTcpListener;
use fahrenheit::AsyncTcpStream;
use futures::io::AsyncReadExt;
use futures::stream::StreamExt;

async fn listen(addr: &str) {
    let addr: SocketAddr = addr.parse().unwrap();
    let listener = AsyncTcpListener::bind(addr).unwrap();
    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        fahrenheit::spawn(process(stream));
    }
}

async fn process(mut stream: AsyncTcpStream) {
    let mut buf = vec![0; 10];
    let _ = stream.read_exact(&mut buf).await;
    println!("{}", String::from_utf8_lossy(&buf));
}

fn main() {
    fahrenheit::run(listen("127.0.0.1:12345"))
}
