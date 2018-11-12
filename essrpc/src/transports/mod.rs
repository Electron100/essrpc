use std::io;
use std::io::{Read, Write};

mod bincode;
mod json;

pub use self::json::JSONTransport;
pub use self::bincode::BincodeTransport;

pub struct ReadWrite<R: Read, W: Write> {
    r: R,
    w: W,
}

impl <R:Read, W: Write> ReadWrite<R, W> {
    pub fn new(r: R, w: W) -> Self {
        ReadWrite{r: r, w: w}
    }
}

impl <R:Read, W: Write> Read for ReadWrite<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.r.read(buf)
    }
}

impl <R:Read, W:Write> Write for ReadWrite<R, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.w.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}
