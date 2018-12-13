use bytes::{Buf, BufMut};
use futures::{future, Async, Future, Poll};
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use std::io::BufRead;

#[derive(Debug)]
pub struct ProxyTcpStream {
    stream: TcpStream,
    remote_addr: SocketAddr,
}

impl ProxyTcpStream {
    pub fn peek(stream: TcpStream) -> impl Future<Item = Self, Error = io::Error> {
        let mut bytes = [0; 107];
        let mut stream = Some(stream);
        future::poll_fn(move || {
            let n = try_ready!(stream.as_mut().unwrap().poll_peek(&mut bytes));
            // TODO: check bytes[..n] for PROXY line
            let mut stream = stream.take().unwrap();
            let mut remote_addr: SocketAddr = stream.peer_addr().unwrap();
            let mut s = String::new();
            match bytes.as_ref().read_line(&mut s) {
                Ok(ln) => {
                    if s.starts_with("PROXY TCP") {
                        // read that line for real (no peeking!)
                        let mut v = vec![0; ln];
                        stream.read_exact(&mut v).unwrap();
                        let mut split = s.split(" ").skip(2);
                        let ip = split.next().unwrap();
                        let port = split.skip(1).next().unwrap();
                        remote_addr = format!("{}:{}", ip, port).parse().unwrap();
                        debug!("using proxy proto, remote addr: {}", remote_addr);
                    }
                }
                Err(e) => error!("error reading PROXY line: {}", e),
            };
            Ok(Async::Ready(ProxyTcpStream {
                stream,
                remote_addr,
            }))
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.remote_addr.clone())
    }
}

impl Read for ProxyTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for ProxyTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl AsyncRead for ProxyTcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.stream.prepare_uninitialized_buffer(buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&TcpStream>::read_buf(&mut &self.stream, buf)
    }
}

impl AsyncWrite for ProxyTcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        <&TcpStream>::shutdown(&mut &self.stream)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&TcpStream>::write_buf(&mut &self.stream, buf)
    }
}
