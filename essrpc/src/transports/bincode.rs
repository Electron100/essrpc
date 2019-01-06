
use std::io::{Read, Write};

use bincode;
use serde::{Serialize, Deserialize};

use crate::{MethodId, PartialMethodId, Result, RPCError, RPCErrorKind,
            ClientTransport, ServerTransport};

/// Transport implementation using Bincode serialization. Can be used
/// over any `Read+Write` channel (local socket, internet socket,
/// pipe, etc). The present implementation is naive with regards to
/// this channel -- no buffering is performed.
/// Enable the "bincode_transport" feature to use this.
pub struct BincodeTransport<C: Read+Write> {
    channel: C
}

impl <C: Read+Write> BincodeTransport<C> {
    pub fn new(channel: C) -> Self {
        BincodeTransport{channel: channel}
    }

    /// Get the underlying read/write channel
    pub fn channel<'a>(&'a self) -> &'a C {
        &self.channel
    }

    fn serialize(&mut self, value: impl Serialize) -> Result<()>{
        bincode::serialize_into(Write::by_ref(&mut self.channel),
                                &value)
            .map_err(|e|
                     RPCError::with_cause(
                         RPCErrorKind::SerializationError, "bincode serialization failure", e))
    }

    fn deserialize<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de> {
        bincode::deserialize_from(
            Read::by_ref(&mut self.channel))
            .map_err(|e|
                     RPCError::with_cause(
                         RPCErrorKind::SerializationError, "bincode deserialization failure", e))
    }
}

impl <C: Read+Write> ClientTransport for BincodeTransport<C> {
    type TXState = ();

    fn tx_begin_call(&mut self, method: MethodId) -> Result<()> {
        self.serialize(method.num)
    }

    fn tx_add_param(&mut self, _name: &'static str, value: impl Serialize,
                    _state: &mut ()) -> Result<()> {
        self.serialize(value)
    }

    fn tx_finalize(&mut self, _state: &mut ()) -> Result<()> {
        Ok(())
    }

    fn rx_response<T>(&mut self, _state: &mut ()) -> Result<T> where
        for<'de> T: Deserialize<'de> {
        self.deserialize()
    }


}
impl <C: Read+Write> ServerTransport for BincodeTransport<C> {
    type RXState = ();
 
    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, ())> {
        let method_id: u32 = self.deserialize()?;
        Ok((PartialMethodId::Num(method_id), ()))
    }
    
    fn rx_read_param<T>(&mut self, _name: &'static str, _state: &mut ()) -> Result<T> where
        for<'de> T: serde::Deserialize<'de> {
        self.deserialize()
    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        self.serialize(value)
    }
}
