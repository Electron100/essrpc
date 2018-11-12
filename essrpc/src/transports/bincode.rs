
use std::io::{Read, Write};

use bincode;
use failure::Fail;
use serde::{Serialize, Deserialize};

use crate::{MethodId, PartialMethodId, Result, Transport};

// todo buffer

pub struct BincodeTransport<C: Read+Write> {
    channel: C
}

impl <C: Read+Write> BincodeTransport<C> {
    pub fn new(channel: C) -> Self {
        BincodeTransport{channel: channel}
    }

    fn convert_error(e: impl Fail) -> failure::Error {
        let e: failure::Error = e.into();
        e.context("json (s|d)erialization failure").into()
    }

    fn serialize(&mut self, value: impl Serialize) -> Result<()>{
        bincode::serialize_into(Write::by_ref(&mut self.channel),
                                &value)
            .map_err(Self::convert_error)
    }

    fn deserialize<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de> {
        bincode::deserialize_from(
            Read::by_ref(&mut self.channel))
            .map_err(Self::convert_error)
    }
}

impl <C: Read+Write> Transport for BincodeTransport<C> {
    type TXState = ();
    type RXState = ();
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

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, ())> {
        let method_id: u32 = self.deserialize()?;
        Ok((PartialMethodId::Num(method_id), ()))
    }
    
    fn rx_read_param<T>(&mut self, _name: &'static str, _state: &mut ()) -> Result<T> where
        for<'de> T: serde::Deserialize<'de> {
        self.deserialize()
    }

    fn rx_response<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de>
    {
        self.deserialize()
    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        self.serialize(value)
    }
}
