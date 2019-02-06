use bytes::{Buf, BufMut};
use futures::{future, Async, Future, Poll};
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use std::io::BufRead;
use std::time::Duration;

#[derive(Debug)]
pub struct ProxyTcpStream {
    pub tls: bool,
    stream: TcpStream,
    remote_addr: SocketAddr,
}

impl ProxyTcpStream {
    pub fn from_tcp_stream(
        stream: TcpStream,
        tls: bool,
    ) -> impl Future<Item = Self, Error = io::Error> {
        let mut bytes = [0; 107];
        stream.set_nodelay(true).ok();
        stream.set_keepalive(Some(Duration::from_secs(10))).ok();
        let mut stream = Some(stream);
        future::poll_fn(move || {
            let _n = try_ready!(stream.as_mut().unwrap().poll_peek(&mut bytes));
            // TODO: check bytes[..n] for PROXY line
            let mut stream = stream.take().unwrap();
            let mut remote_addr: SocketAddr =
                stream.peer_addr().unwrap_or("0.0.0.0:0".parse().unwrap());
            let mut s = String::new();
            match bytes.as_ref().read_line(&mut s) {
                Ok(ln) => {
                    if s.starts_with("PROXY TCP") {
                        // read that line for real (no peeking!)
                        let mut v = vec![0; ln];
                        stream.read_exact(&mut v).unwrap();
                        let mut split = s.split(" ").skip(2);
                        let ip = split.next().unwrap_or("0.0.0.0");
                        let port = split.skip(1).next().unwrap_or("0");
                        remote_addr = format!("{}:{}", ip, port).parse().unwrap();
                        debug!("using proxy proto, remote addr: {}", remote_addr);
                    }
                }
                Err(e) => debug!("error reading PROXY protocol line: {}", e),
            };
            Ok(Async::Ready(ProxyTcpStream {
                tls,
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
