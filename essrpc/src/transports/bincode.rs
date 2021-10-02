use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::io;
use std::io::{Read, Write};
use tokio_util::codec::LengthDelimitedCodec;

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

fn read_msg_len(mut r: impl Read) -> Result<usize> {
    let mut msg_len_bytes = [0u8; 4];
    r.read_exact(&mut msg_len_bytes)?;
    Ok(u32::from_le_bytes(msg_len_bytes) as usize)
}

fn write_msg_len(mut w: impl Write, len: usize) -> Result<()> {
    w.write_all(&(len as u32).to_le_bytes())?;
    Ok(())
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
        write_msg_len(&mut self.channel, state.len())?;
        self.channel.write_all(&state)?;
        self.flush()?;
        Ok(())
    }

    fn rx_response<T>(&mut self, _state: ()) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let msg_len = read_msg_len(&mut self.channel)?;
        let mut buffer = Vec::new();
        buffer.resize(msg_len, 0);
        self.channel.read_exact(buffer.as_mut_slice())?;
        deserialize(buffer.as_slice())
    }
}

pub struct VecReader {
    v: Vec<u8>,
    pos: usize,
}
impl VecReader {
    fn new(v: Vec<u8>) -> Self {
        VecReader { v, pos: 0 }
    }
}
impl std::io::Read for VecReader {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        let wanted = buf.len();
        let avail = self.v.len() - self.pos;
        if avail == 0 {
            return Ok(0);
        }
        let written = match buf.write(&self.v.as_slice()[self.pos..])? {
            0 => wanted,
            n => n,
        };
        self.pos += written;
        Ok(written)
    }
}

impl<C: Read + Write> ServerTransport for BincodeTransport<C> {
    type RXState = VecReader;

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, Self::RXState)> {
        let msg_len = read_msg_len(&mut self.channel)?;
        let mut buffer = Vec::new();
        buffer.resize(msg_len, 0);
        self.channel.read_exact(buffer.as_mut_slice())?;
        let mut reader = VecReader::new(buffer);
        let method_id: u32 = deserialize(&mut reader)?;
        Ok((PartialMethodId::Num(method_id), reader))
    }

    fn rx_read_param<T>(&mut self, _name: &'static str, state: &mut Self::RXState) -> Result<T>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        deserialize(state)
    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        let mut msg: Vec<u8> = Vec::new();
        serialize(&mut msg, value)?;
        write_msg_len(&mut self.channel, msg.len())?;
        self.channel.write_all(&msg)?;
        self.flush()?;
        Ok(())
    }
}

#[cfg(feature = "async_client")]
mod async_client {
    use super::*;
    use crate::AsyncClientTransport;
    use futures::{SinkExt, StreamExt};
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_util::codec::Framed;

    /// Like BincodeTransport except for use as
    /// AsyncClientTransport.Can be used over any `AsyncRead+AsyncWrite+Send` channel
    /// (local socket, internet socket, pipe, etc).
    pub struct BincodeAsyncClientTransport<C: AsyncRead + AsyncWrite + Send> {
        channel: Framed<C, LengthDelimitedCodec>,
    }

    impl<C: AsyncRead + AsyncWrite + Send> BincodeAsyncClientTransport<C> {
        /// Create an AsyncBincodeTransport.
        pub fn new(channel: C) -> Self {
            BincodeAsyncClientTransport {
                channel: Framed::new(
                    channel,
                    LengthDelimitedCodec::builder()
                        .little_endian()
                        .max_frame_length(usize::MAX)
                        .new_codec(),
                ),
            }
        }
    }

    #[async_trait]
    impl<C: AsyncRead + AsyncWrite + Send + Unpin> AsyncClientTransport
        for BincodeAsyncClientTransport<C>
    {
        type TXState = Vec<u8>;
        type FinalState = ();

        async fn tx_begin_call(&mut self, method: MethodId) -> Result<Self::TXState> {
            let mut state = Vec::new();
            serialize(&mut state, method.num)?;
            Ok(state)
        }

        async fn tx_add_param(
            &mut self,
            _name: &'static str,
            value: impl Serialize + Send + 'async_trait,
            state: &mut Self::TXState,
        ) -> Result<()> {
            serialize(state, value)
        }

        async fn tx_finalize(&mut self, state: Self::TXState) -> Result<()> {
            self.channel.send(state.into()).await?;
            Ok(())
        }

        async fn rx_response<T>(&mut self, _state: ()) -> Result<T>
        where
            for<'de> T: Deserialize<'de>,
        {
            let msg = self.channel.next().await.unwrap_or_else(|| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Could not rx response, unexpcted EOF",
                ))
            })?;
            deserialize(&*msg)
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::BincodeAsyncClientTransport;
