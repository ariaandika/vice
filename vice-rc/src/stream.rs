//! share stream between tasks
use std::io;
use bytes::{Buf, Bytes, BytesMut};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::{mpsc::{self, error::TrySendError}, oneshot}};

use crate::body::ResBody;

enum StreamMessage {
    Read {
        tx: oneshot::Sender<io::Result<(usize,BytesMut)>>,
        buffer: BytesMut,
    },
    ReadExact {
        len: usize,
        tx: oneshot::Sender<io::Result<BytesMut>>,
        buffer: BytesMut,
    },
    Write {
        tx: oneshot::Sender<io::Result<()>>,
        head: Bytes,
        body: ResBody,
    },
}

/// share tcp stream via channel
pub fn new_task(stream: TcpStream) -> StreamHandle {
    let (send,recv) = mpsc::channel::<StreamMessage>(2);
    tokio::spawn(task(stream, recv));
    StreamHandle { send }
}

async fn task(mut stream: TcpStream, mut recv: mpsc::Receiver<StreamMessage>) {
    use StreamMessage::*;

    while let Some(message) = recv.recv().await {
        match message {
            Read { tx, mut buffer } => {
                let _ = match stream.read_buf(&mut buffer).await {
                    Ok(ok) => tx.send(Ok((ok,buffer))),
                    Err(err) => tx.send(Err(err)),
                };
            }
            ReadExact { len, tx, mut buffer } => {
                let _ = match stream.read_exact(&mut buffer[..len]).await {
                    Ok(_) => tx.send(Ok(buffer)),
                    Err(err) => tx.send(Err(err)),
                };
            }
            Write { tx, head, body } => {
                let _ = match stream.write_all_buf(&mut Buf::chain(head, body.as_ref())).await {
                    Ok(()) => tx.send(Ok(())),
                    Err(err) => tx.send(Err(err)),
                };
            }
        }
    }
}

pin_project_lite::pin_project! {
    /// wrap oneshot::Recevier to map error as io error
    pub struct StreamFuture<T> {
        #[pin]
        recv: oneshot::Receiver<T>,
    }
}

impl<T> StreamFuture<T> {
    fn new(recv: oneshot::Receiver<T>) -> StreamFuture<T> {
        Self { recv }
    }
}

macro_rules! ch_to_io_err {
    ($err:ident) => {
        io::Error::new(io::ErrorKind::Other, format!("stream task error: {}",$err))
    };
}

impl<T> Future for StreamFuture<io::Result<T>> {
    type Output = io::Result<T>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        use std::task::Poll::*;

        match self.project().recv.poll(cx) {
            std::task::Poll::Ready(result) => {
                match result {
                    Ok(io_result) => match io_result {
                        Ok(ok) => Ready(Ok(ok)),
                        Err(err) => Ready(Err(ch_to_io_err!(err)))
                    }
                    Err(err) => {
                        Ready(Err(ch_to_io_err!(err)))
                    },
                }
            }
            Pending => Pending,
        }
    }
}

/// clonable handle of tcp stream task
pub struct StreamHandle {
    send: mpsc::Sender<StreamMessage>,
}

macro_rules! send {
    ($self:ident,$variant:ident { $($args:ident,)* }) => {{
        use StreamMessage::*;

        let (tx,rx) = oneshot::channel();
        match $self.send.try_send($variant { tx, $($args,)* }) {
            Ok(()) => StreamFuture::new(rx),
            Err(err) => {
                let ch_err = ch_to_io_err!(err);
                let tx = match err {
                    TrySendError::Full($variant { tx, .. }) => tx,
                    TrySendError::Closed($variant { tx, .. }) => tx,
                    _ => unreachable!(),
                };
                tx.send(Err(ch_err));
                StreamFuture::new(rx)
            },
        }
    }};
}

impl StreamHandle {
    pub fn read(&self, buffer: BytesMut) -> StreamFuture<io::Result<(usize, BytesMut)>> {
        send!(self,Read { buffer, })
    }

    pub fn read_exact(&self, len: usize, buffer: BytesMut) -> StreamFuture<io::Result<BytesMut>> {
        send!(self,ReadExact { len, buffer, })
    }

    pub fn write(&self, head: Bytes, body: ResBody) -> StreamFuture<io::Result<()>> {
        send!(self,Write { head, body, })
    }
}

