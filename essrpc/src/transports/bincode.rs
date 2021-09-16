use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::io;
use std::io::{Read, Write};

use crate::{
    ClientTransport, MethodId, PartialMethodId, RPCError, RPCErrorKind, Result, ServerTransport,
};

fn serialize(w: impl Write, value: impl Serialize) -> Result<()> {
    bincode::serialize_into(w, &value).map_err(|e| {
        RPCError::with_cause(
            RPCErrorKind::SerializationError,
            "bincode serialization failure",
            e,
        )
    })
}

fn deserialize<T>(r: impl Read) -> Result<T>
where
    for<'de> T: Deserialize<'de>,
{
    bincode::deserialize_from(r).map_err(|e| {
        if let bincode::ErrorKind::Io(e) = e.as_ref() {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return RPCError::new(
                    RPCErrorKind::TransportEOF,
                    "EOF during bincode deserialization",
                );
            }
        }
        RPCError::with_cause(
            RPCErrorKind::SerializationError,
            "bincode deserialization failure",
            e,
        )
    })
}

/// Transport implementation using Bincode serialization. Can be used
/// over any `Read+Write` channel (local socket, internet socket,
/// pipe, etc). The present implementation is naive with regards to
/// this channel -- no buffering is performed.
/// Enable the "bincode_transport" feature to use this.
pub struct BincodeTransport<C: Read + Write> {
    channel: C,
}

impl<C: Read + Write> BincodeTransport<C> {
    pub fn new(channel: C) -> Self {
        BincodeTransport { channel }
    }

    /// Get the underlying read/write channel
    pub fn channel(&self) -> &C {
        &self.channel
    }

    fn deserialize<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        deserialize(Read::by_ref(&mut self.channel))
    }

    fn flush(&mut self) -> Result<()> {
        self.channel.flush().map_err(|e| {
            RPCError::with_cause(
                RPCErrorKind::SerializationError,
                "cannot flush underlying channel",
                e,
            )
        })
    }
}

impl<C: Read + Write> ClientTransport for BincodeTransport<C> {
    type TXState = Vec<u8>;
    type FinalState = ();

    fn tx_begin_call(&mut self, method: MethodId) -> Result<Vec<u8>> {
        let mut state = Vec::new();
        serialize(&mut state, method.num)?;
        Ok(state)
    }

    fn tx_add_param(
        &mut self,
        _name: &'static str,
        value: impl Serialize,
        state: &mut Vec<u8>,
    ) -> Result<()> {
        serialize(state, value)
    }

    fn tx_finalize(&mut self, state: Vec<u8>) -> Result<()> {
        let header = FrameHeader::new(state.len());
        self.channel.write_all(&header.as_bytes())?;
        self.channel.write_all(&state)?;
        self.flush()?;
        Ok(())
    }

    fn rx_response<T>(&mut self, _state: ()) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let mut header = [0u8; FrameHeader::HEADER_LEN];
        self.channel.read_exact(&mut header)?;
        let mut buffer = Vec::new();
        buffer.resize(FrameHeader::from_bytes(header)?.len(), 0);
        self.channel.read_exact(buffer.as_mut_slice())?;
        deserialize(buffer.as_slice())
    }
}
impl<C: Read + Write> ServerTransport for BincodeTransport<C> {
    type RXState = ();

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, ())> {
        eprintln!("rx begin call");
        let mut header = [0u8; FrameHeader::HEADER_LEN];
        self.channel.read_exact(&mut header)?;
        eprintln!("rx read exact");
        FrameHeader::from_bytes(header)?;
        // todo check header
        let method_id: u32 = self.deserialize()?;
        Ok((PartialMethodId::Num(method_id), ()))
    }

    fn rx_read_param<T>(&mut self, _name: &'static str, _state: &mut ()) -> Result<T>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        self.deserialize()
    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        let mut msg: Vec<u8> = Vec::new();
        serialize(&mut msg, value)?;
        let header = FrameHeader::new(msg.len());
        self.channel.write_all(&header.as_bytes())?;
        self.channel.write_all(&msg)?;
        self.flush()?;
        Ok(())
    }
}

struct FrameHeader {
    msg_len: u32,
}
impl FrameHeader {
    const HEADER_LEN: usize = 10;
    fn new(len: usize) -> Self {
        FrameHeader {
            msg_len: (len as u32),
        }
    }
    fn as_bytes(&self) -> [u8; Self::HEADER_LEN] {
        let mut frame_header = [0u8; Self::HEADER_LEN];
        frame_header[..6].copy_from_slice(b"ESSRPC");
        frame_header[6..].copy_from_slice(&(self.msg_len.to_le_bytes()));
        frame_header
    }
    fn from_bytes(bytes: [u8; Self::HEADER_LEN]) -> Result<Self> {
        // todo validate initial bytes
        if bytes[0..6] != *b"ESSRPC" {
            return Err(RPCError::new(
                RPCErrorKind::TransportError,
                "IO error in transport",
            ));
        }
        let len = u32::from_le_bytes(bytes[6..10].try_into()?);
        Ok(Self::new(len as usize))
    }
    fn len(&self) -> usize {
        self.msg_len as usize
    }
}

#[cfg(feature = "async_client")]
mod async_client {
    use super::*;
    use crate::AsyncClientTransport;
    use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    /// Like BincodeTransport except for use as
    /// AsyncClientTransport.Can be used over any `AsyncRead+AsyncWrite+Send` channel
    /// (local socket, internet socket, pipe, etc).
    pub struct BincodeAsyncClientTransport<C: AsyncRead + AsyncWrite + Send> {
        channel: C,
    }

    impl<C: AsyncRead + AsyncWrite + Send> BincodeAsyncClientTransport<C> {
        /// Create an AsyncBincodeTransport.
        pub fn new(channel: C) -> Self {
            BincodeAsyncClientTransport { channel }
        }
    }

    #[async_trait]
    impl<C: AsyncRead + AsyncWrite + Send + Unpin> AsyncClientTransport
        for BincodeAsyncClientTransport<C>
    {
        type TXState = Vec<u8>;
        type FinalState = ();

        async fn tx_begin_call(&mut self, method: MethodId) -> Result<Vec<u8>> {
            let mut state = Vec::new();
            serialize(&mut state, method.num)?;
            Ok(state)
        }

        async fn tx_add_param(
            &mut self,
            _name: &'static str,
            value: impl Serialize + Send + 'async_trait,
            state: &mut Vec<u8>,
        ) -> Result<()> {
            serialize(state, value)
        }

        async fn tx_finalize(&mut self, state: Vec<u8>) -> Result<()> {
            let header = FrameHeader::new(state.len());
            self.channel.write(&header.as_bytes()).await?;
            self.channel.write(&state).await?;
            self.channel.flush().await?;
            Ok(())
        }

        async fn rx_response<T>(&mut self, _state: ()) -> Result<T>
        where
            for<'de> T: Deserialize<'de>,
        {
            let mut header = [0u8; FrameHeader::HEADER_LEN];
            self.channel.read_exact(&mut header).await?;
            let mut buffer = Vec::new();
            buffer.resize(FrameHeader::from_bytes(header)?.len(), 0);
            eprintln!(
                "Initialized buffer with size {} or {}",
                buffer.len(),
                buffer.as_mut_slice().len()
            );
            self.channel.read_exact(buffer.as_mut_slice()).await?;
            deserialize(buffer.as_slice())
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::BincodeAsyncClientTransport;
