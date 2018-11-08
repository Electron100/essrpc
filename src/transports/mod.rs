use std::io::Read;
use std::io::Write;

use crate::Result;
use crate::Transport;

pub struct ReadWriteTransport<T: Read + Write> {
    channel: T
}

impl <T: Read + Write> ReadWriteTransport<T> {
    pub fn new(channel: T) -> Self {
        ReadWriteTransport{channel: channel}
    }
}

impl <T: Read + Write> Transport for ReadWriteTransport<T> {
    type Wire = Vec<u8>;
    
    fn send(&mut self, request: Vec<u8>) -> Result<()> {
        self.channel.write_all(&request)?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Self::Wire> {
        let mut result = Vec::new();
        self.channel.read_to_end(&mut result)?;
        Ok(result)
    }
}
