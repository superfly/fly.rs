use bytes::{Buf, BufMut};
use futures::Poll;
use std::io::{self, Read, Write};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::proxy;

pub enum Conn {
    Tls(tokio_openssl::SslStream<proxy::ProxyTcpStream>),
    Tcp(proxy::ProxyTcpStream),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.read(buf),
            Conn::Tls(ssl_stream) => ssl_stream.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.write(buf),
            Conn::Tls(ssl_stream) => ssl_stream.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.flush(),
            Conn::Tls(ssl_stream) => ssl_stream.flush(),
        }
    }
}

impl AsyncRead for Conn {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.prepare_uninitialized_buffer(buf),
            Conn::Tls(ssl_stream) => ssl_stream.prepare_uninitialized_buffer(buf),
        }
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.read_buf(buf),
            Conn::Tls(ssl_stream) => ssl_stream.read_buf(buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.shutdown(),
            Conn::Tls(ssl_stream) => ssl_stream.shutdown(),
        }
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Conn::Tcp(proxy_stream) => proxy_stream.write_buf(buf),
            Conn::Tls(ssl_stream) => ssl_stream.write_buf(buf),
        }
    }
}
