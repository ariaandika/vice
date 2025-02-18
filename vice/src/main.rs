use std::{io, net::TcpListener as StdTcpListener};

use tokio::{io::AsyncReadExt, net::TcpListener};
use vice::service::{connection::{Connection, ConnectionBuffer}, service_fn, Service};


const ADDR: &str = "0.0.0.0:3000";

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("failed to bind tcp: {0}")]
    Tcp(io::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

fn main() -> Result<(), Error> {
    let tcp = StdTcpListener::bind(ADDR).map_err(Error::Tcp)?;
    tcp.set_nonblocking(true)?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let tcp = TcpListener::from_std(tcp)?;

            loop {
                let (stream, _addr) = match tcp.accept().await {
                    Ok(ok) => ok,
                    Err(err) => {
                        eprintln!("{err}");
                        continue;
                    },
                };

                tokio::spawn(Connection::new(service_fn(handle)).call(stream));
            }
        })
}

async fn handle(mut conn: ConnectionBuffer) -> Result<Vec<u8>, Error> {
    let mut read = false;
    let mut headers = [httparse::EMPTY_HEADER;24];
    let mut buffer = conn.buffer_handle();
    let mut stream = conn.stream_handle();

    let (request, body_offset) = loop {

        if read {
            stream.read_buf(&mut *buffer).await.unwrap();
        }

        let mut request = httparse::Request::new(&mut headers);

        let body_offset = match request.parse(buffer.as_static()).unwrap() {
            httparse::Status::Complete(ok) => ok,
            httparse::Status::Partial => {
                read = true;
                continue;
            }
        };

        break (request, body_offset);
    };

    use std::str::from_utf8 as to_str;

    dbg!(request.headers);

    if let Some(expected_len) = headers
        .iter()
        .find(|e|e.name.eq_ignore_ascii_case("content-length"))
        .and_then(|e|to_str(e.value).ok()?.parse::<usize>().ok())
    {
        while (buffer.len() - body_offset) < expected_len {
            stream.read_buf(&mut *buffer).await.unwrap();
        }

        let body = &buffer[body_offset..body_offset + expected_len];
        dbg!(to_str(body)).ok();
    }

    Ok(Vec::from(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"))
}


