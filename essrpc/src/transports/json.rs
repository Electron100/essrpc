use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::value::Value;
use serde_json::json;
use uuid::Uuid;

use crate::{MethodId, PartialMethodId, Result, RPCError, RPCErrorKind, Transport};


pub struct JTXState {
    method: &'static str,
    params: Value
}

pub struct JRXState {
    json: Value
}

pub struct JSONTransport<C: Read+Write> {
    channel: C
}

impl <C: Read+Write> JSONTransport<C> {
    pub fn new(channel: C) -> Self {
        JSONTransport{channel: channel}
    }

    fn convert_error(e: impl std::error::Error) -> RPCError {
        RPCError::with_cause(RPCErrorKind::SerializationError,
                             "json serialization or deserialization failed", e)
    }

    fn from_channel<T>(&mut self) -> Result<T> where
        for<'de> T: serde::Deserialize<'de> {

        let read = serde_json::de::IoRead::new(Read::by_ref(&mut self.channel));
        let mut de = serde_json::de::Deserializer::new(read);
        serde::de::Deserialize::deserialize(&mut de)
            .map_err(Self::convert_error)
    }
}
impl <C: Read+Write> Transport for JSONTransport<C> {
    type TXState = JTXState;
    type RXState = JRXState;
   
    fn tx_begin_call(&mut self, method: MethodId) -> Result<JTXState> {
        Ok(JTXState{method: method.name, params: json!({})})
    }

    fn tx_add_param(&mut self, name: &'static str, value: impl Serialize, state: &mut JTXState) -> Result<()> {
        state.params.as_object_mut().unwrap()
            .insert(name.to_string(),
                    serde_json::to_value(value).map_err(Self::convert_error)?);
        Ok(())
    }

    fn tx_finalize(&mut self, state: &mut JTXState) -> Result<()> {
        serde_json::to_writer(Write::by_ref(&mut self.channel), &json!({
            "jsonrpc": "2.0",
            "method": state.method,
            "params": state.params,
            "id": format!("{}", Uuid::new_v4())
        })).map_err(Self::convert_error)
    }

    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, JRXState)> {
        let value: Value = self.from_channel()?;
        let method = value.get("method")
            .ok_or(RPCError::new(RPCErrorKind::SerializationError, "json is not expected object"))?
            .as_str()
            .ok_or(RPCError::new(RPCErrorKind::SerializationError, "json method was not string"))?
            .to_string();
        Ok((PartialMethodId::Name(method), JRXState{json: value}))
    }
    
    fn rx_read_param<T>(&mut self, name: &'static str, state: &mut JRXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de> {
        
        let param_val = state.json.get("params")
            .ok_or(RPCError::new(RPCErrorKind::SerializationError, "json is not expected object"))?
            .get(name)
            .ok_or(RPCError::new(RPCErrorKind::SerializationError,
                                 format!("parameters do not contain {}", name)))?;
        return serde_json::from_value(param_val.clone()).map_err(Self::convert_error);
    }

    fn rx_response<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de>
    {
        self.from_channel()

    }

    fn tx_response(&mut self, value: impl Serialize) -> Result<()> {
        serde_json::to_writer(Write::by_ref(&mut self.channel), &value)
            .map_err(Self::convert_error)
    }
    
}
