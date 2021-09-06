use async_trait::async_trait;
use serde::{Deserialize, Serialize};
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

    fn serialize(&mut self, value: impl Serialize) -> Result<()> {
        serialize(Write::by_ref(&mut self.channel), value)
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
    type TXState = ();
    type FinalState = ();

    fn tx_begin_call(&mut self, method: MethodId) -> Result<()> {
        self.serialize(method.num)
    }

    fn tx_add_param(
        &mut self,
        _name: &'static str,
        value: impl Serialize,
        _state: &mut (),
    ) -> Result<()> {
        self.serialize(value)
    }

    fn tx_finalize(&mut self, _state: ()) -> Result<()> {
        self.flush()?;
        Ok(())
    }

    fn rx_response<T>(&mut self, _state: ()) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        self.deserialize()
    }
}
impl<C: Read + Write> ServerTransport for BincodeTransport<C> {
    type RXState = ();

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, ())> {
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
        let res = self.serialize(value);
        self.flush()?;
        res
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
            self.channel.write(&state).await?;
            self.channel.flush().await?;
            Ok(())
        }

        async fn rx_response<T>(&mut self, _state: ()) -> Result<T>
        where
            for<'de> T: Deserialize<'de>,
        {
            // Note, there are a couple limitations here that we should potentially address in the future
            // 1. Arbitrary limit on return type size
            // 2. For stream-based channels such as TCP, if the return
            //    type size is greater than the TCP segment size, we
            //    will likely splice the data and the deserialize
            //    would fail. We need to add our own frame concept on top of the channel.
            //    or switch to using Source/Sink instead of Read/Write
            let mut buffer = [0u8; 1024];
            self.channel.read(&mut buffer).await?;
            deserialize(&buffer as &[u8])
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::BincodeAsyncClientTransport;
