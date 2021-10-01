use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::Value;
use std::io::{Read, Write};
use uuid::Uuid;

use crate::{
    ClientTransport, MethodId, PartialMethodId, RPCError, RPCErrorKind, Result, ServerTransport,
};

pub struct JTXState {
    method: &'static str,
    params: Value,
}

pub struct JRXState {
    json: Value,
}

/// Transport implementation over JSON-RPC. Can be used over any
/// `Read+Write` channel (local socket, internet socket, pipe,
/// etc). Enable the "json_transport" feature to use this.
pub struct JSONTransport<C: Read + Write> {
    channel: C,
}

impl<C: Read + Write> JSONTransport<C> {
    pub fn new(channel: C) -> Self {
        JSONTransport { channel }
    }

    /// Get the underlying read/write channel
    pub fn channel(&self) -> &C {
        &self.channel
    }

    // Deserialize a value from the channel
    fn read_from_channel<T>(&mut self) -> Result<T>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        read_value_from_json(Read::by_ref(&mut self.channel))
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
impl<C: Read + Write> ClientTransport for JSONTransport<C> {
    type TXState = JTXState;
    type FinalState = ();

    fn tx_begin_call(&mut self, method: MethodId) -> Result<JTXState> {
        Ok(begin_call(method))
    }

    fn tx_add_param(
        &mut self,
        name: &'static str,
        value: impl Serialize,
        state: &mut JTXState,
    ) -> Result<()> {
        add_param(name, value, state)
    }

    fn tx_finalize(&mut self, state: JTXState) -> Result<()> {
        serde_json::to_writer(Write::by_ref(&mut self.channel), &value_for_state(&state))
            .map_err(convert_error)?;
        self.flush()
    }

    fn rx_response<T>(&mut self, _state: ()) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        self.read_from_channel()
    }
}

fn convert_error(e: impl std::error::Error) -> RPCError {
    RPCError::with_cause(
        RPCErrorKind::SerializationError,
        "json serialization or deserialization failed",
        e,
    )
}

fn begin_call(method: MethodId) -> JTXState {
    JTXState {
        method: method.name,
        params: json!({}),
    }
}

fn value_for_state(state: &JTXState) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": state.method,
        "params": state.params,
        "id": format!("{}", Uuid::new_v4())
    })
}

fn add_param(name: &'static str, value: impl Serialize, state: &mut JTXState) -> Result<()> {
    state.params.as_object_mut().unwrap().insert(
        name.to_string(),
        serde_json::to_value(value).map_err(convert_error)?,
    );
    Ok(())
}

fn read_value_from_json<T, R>(reader: R) -> Result<T>
where
    for<'de> T: serde::Deserialize<'de>,
    R: Read,
{
    let read = serde_json::de::IoRead::new(reader);
    let mut de = serde_json::de::Deserializer::new(read);
    serde::de::Deserialize::deserialize(&mut de).map_err(|e| {
        if e.classify() == serde_json::error::Category::Eof {
            RPCError::new(
                RPCErrorKind::TransportEOF,
                "EOF during json deserialization",
            )
        } else {
            convert_error(e)
        }
    })
}

impl<C: Read + Write> ServerTransport for JSONTransport<C> {
    type RXState = JRXState;

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, JRXState)> {
        let value: Value = self.read_from_channel()?;
        let method = value
            .get("method")
            .ok_or_else(|| {
                RPCError::new(
                    RPCErrorKind::SerializationError,
                    "json is not expected object",
                )
            })?
            .as_str()
            .ok_or_else(|| {
                RPCError::new(
                    RPCErrorKind::SerializationError,
                    "json method was not string",
                )
            })?
            .to_string();
        Ok((PartialMethodId::Name(method), JRXState { json: value }))
    }

    fn rx_read_param<T>(&mut self, name: &'static str, state: &mut JRXState) -> Result<T>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        let param_val = state
            .json
            .get("params")
            .ok_or_else(|| {
                RPCError::new(
                    RPCErrorKind::SerializationError,
                    "json is not expected object",
                )
            })?
            .get(name)
            .ok_or_else(|| {
                RPCError::new(
                    RPCErrorKind::SerializationError,
                    format!("parameters do not contain {}", name),
                )
            })?;
        serde_json::from_value(param_val.clone()).map_err(convert_error)
    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        let res = serde_json::to_writer(Write::by_ref(&mut self.channel), &value)
            .map_err(convert_error)?;
        self.flush()?;
        Ok(res)
    }
}

#[cfg(feature = "async_client")]
mod async_client {
    use super::*;
    use crate::AsyncClientTransport;
    use bytes::{BufMut, Bytes, BytesMut};
    use futures::{Sink, SinkExt, Stream, StreamExt};
    use std::io::Result as IoResult;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_util::codec::Framed;

    /// Like JSONTransport except for use as AsyncClientTransport.
    pub struct JSONAsyncClientTransport<C>
    where
        C: Sink<Bytes>,
        C: Stream,
    {
        channel: C,
    }

    impl<C: Sink<Bytes> + Stream> JSONAsyncClientTransport<C> {
        /// Create an AsyncJSONTransport.
        pub fn new(channel: C) -> Self {
            JSONAsyncClientTransport { channel }
        }
    }

    impl<A> JSONAsyncClientTransport<Framed<A, JSONCodec>>
    where
        A: AsyncRead + AsyncWrite,
    {
        pub fn new_unframed(channel: A) -> Self
        where
            A: AsyncRead + AsyncWrite,
        {
            Self::new(Framed::new(channel, JSONCodec::new()))
        }
    }

    #[async_trait]
    impl<C> AsyncClientTransport for JSONAsyncClientTransport<C>
    where
        C: Sink<Bytes, Error = std::io::Error>,
        C: Stream<Item = std::result::Result<BytesMut, std::io::Error>>,
        C: Send + Unpin,
    {
        type TXState = JTXState;
        type FinalState = ();

        async fn tx_begin_call(&mut self, method: MethodId) -> Result<JTXState> {
            Ok(begin_call(method))
        }

        async fn tx_add_param(
            &mut self,
            name: &'static str,
            value: impl Serialize + Send + 'async_trait,
            state: &mut JTXState,
        ) -> Result<()> {
            add_param(name, value, state)
        }

        async fn tx_finalize(&mut self, state: JTXState) -> Result<()> {
            let j = serde_json::to_vec(&value_for_state(&state)).map_err(convert_error)?;
            self.channel.send(j.into()).await?;
            self.channel.flush().await?;
            Ok(())
        }

        async fn rx_response<T>(&mut self, _state: ()) -> Result<T>
        where
            for<'de> T: Deserialize<'de>,
        {
            let msg: BytesMut = self.channel.next().await.unwrap_or_else(|| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Could not rx response, unexpcted EOF",
                ))
            })?;
            read_value_from_json(&*msg)
        }
    }

    /// Codec which maps bytes to bytes but only decodes valid
    /// json. Cannot handle a true stream of json objects one after
    /// another with no delimeters, but essrpc always has an messages
    /// from each side.
    pub struct JSONCodec {}
    impl JSONCodec {
        fn new() -> Self {
            JSONCodec {}
        }
    }
    impl tokio_util::codec::Encoder<Bytes> for JSONCodec {
        type Error = std::io::Error;
        fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> IoResult<()> {
            dst.put(item);
            Ok(())
        }
    }
    impl tokio_util::codec::Decoder for JSONCodec {
        type Item = BytesMut;
        type Error = std::io::Error;
        fn decode(&mut self, src: &mut BytesMut) -> IoResult<Option<Self::Item>> {
            let s = match std::str::from_utf8(src) {
                Ok(s) => s,
                Err(_) => return Ok(None), // we might have spliced a sequence, so not a fatal error
            };
            match json::parse(s) {
                // Ok, we're done. Remove the bytes from the buffer, return them
                Ok(_) => Ok(Some(src.split())),
                // Unexpected end of json just means we haven't read enough bytes yet
                Err(json::Error::UnexpectedEndOfJson) => Ok(None),
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            }
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::JSONAsyncClientTransport;
