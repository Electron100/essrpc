use std::io;
use std::io::{Read, Write};

use bincode;
use serde::{Deserialize, Serialize};

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
        self.serialize(value)
    }
}

#[cfg(feature = "async_client")]
mod async_client {
    use super::*;
    use crate::AsyncClientTransport;
    use futures::{future, Future};
    use std::ops::Deref;

    type FutureBytes = Box<dyn Future<Item = Vec<u8>, Error = RPCError>>;

    /// Like BincodeTransport except for use as AsyncClientTransport.
    pub struct BincodeAsyncClientTransport<F>
    where
        F: Fn(Vec<u8>) -> FutureBytes,
    {
        transact: F,
    }

    impl<F> BincodeAsyncClientTransport<F>
    where
        F: Fn(Vec<u8>) -> FutureBytes,
    {
        /// Create an AsyncBincodeTransport. `transact` must be a
        /// function which given the raw bytes to transmit to the server,
        /// returns a future representing the raw bytes returned from the server.
        pub fn new(transact: F) -> Self {
            BincodeAsyncClientTransport { transact }
        }
    }

    impl<F> AsyncClientTransport for BincodeAsyncClientTransport<F>
    where
        F: Fn(Vec<u8>) -> FutureBytes,
    {
        type TXState = Vec<u8>;
        type FinalState = FutureBytes;

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

        fn tx_finalize(&mut self, state: Vec<u8>) -> Result<FutureBytes> {
            Ok((self.transact)(state))
        }

        fn rx_response<T>(
            &mut self,
            state: FutureBytes,
        ) -> Box<dyn Future<Item = T, Error = RPCError>>
        where
            for<'de> T: Deserialize<'de>,
            T: 'static,
        {
            Box::new(state.and_then(|data: Vec<u8>| {
                let ret = deserialize(data.deref());
                match ret {
                    Ok(val) => future::result(val),
                    Err(e) => future::err(e),
                }
            }))
        }
    }
}

#[cfg(feature = "async_client")]
pub use self::async_client::BincodeAsyncClientTransport;
