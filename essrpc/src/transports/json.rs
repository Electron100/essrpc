use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::Value;
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
            .map_err(convert_error)
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
        println!("classification {:?}", e.classify());
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
        serde_json::to_writer(Write::by_ref(&mut self.channel), &value).map_err(convert_error)
    }
}

#[cfg(feature = "async_client")]
mod async_client {
    use super::*;
    use crate::{AsyncClientTransport, BoxFuture};
    use std::ops::Deref;
    use futures::{Future, TryFutureExt, FutureExt};

		type FutureBytes = BoxFuture<Vec<u8>, RPCError>;
		
		/// Like JSONTransport except for use as AsyncClientTransport.
    pub struct JSONAsyncClientTransport<F, FT>
    where
        F: Fn(Vec<u8>) -> FT,
				FT: Future<Output = Result<Vec<u8>>>
    {
        transact: F,
    }

    impl<F, FT> JSONAsyncClientTransport<F, FT>
    where
				F: Fn(Vec<u8>) -> FT,
				FT: Future<Output = Result<Vec<u8>>>
    {
        /// Create an AsyncJSONTransport. `transact` must be a
        /// function which given the raw bytes to transmit to the server,
        /// returns a future representing the raw bytes returned from the server.
        pub fn new(transact: F) -> Self {
            JSONAsyncClientTransport { transact }
        }
    }

    impl<F, FT> AsyncClientTransport for JSONAsyncClientTransport<F, FT>
    where
				F: Fn(Vec<u8>) -> FT,
				FT: Future<Output = Result<Vec<u8>>> + 'static
    {
        type TXState = JTXState;
        type FinalState = FutureBytes;

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

        fn tx_finalize(&mut self, state: JTXState) -> Result<FutureBytes> {
            let j = serde_json::to_vec(&value_for_state(&state)).map_err(convert_error)?;
            Ok((self.transact)(j).boxed_local())
        }

        fn rx_response<T>(
            &mut self,
            state: FutureBytes,
        ) -> BoxFuture<T, RPCError>
        where
            for<'de> T: Deserialize<'de>,
            T: 'static,
        {
            println!("rx response");
            state.and_then(|data| async move {
                println!("json is {}", std::str::from_utf8(data.deref()).unwrap());
                println!("result type is {}", std::any::type_name::<T>());
                read_value_from_json(data.deref())
            }).boxed_local()
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::JSONAsyncClientTransport;
