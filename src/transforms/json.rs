use failure::Fail;
use serde::{Deserialize, Serialize};
use serde_json::value::Value;
use serde_json::json;
use uuid::Uuid;

use crate::Result;
use crate::RPCError;
use crate::Transform;

pub struct JTXState {
    method: &'static str,
    params: Value
}

pub struct JRXState {
    json: Value
}

pub struct JSONTransform {}

impl JSONTransform {
    pub fn new() -> Self {
        JSONTransform{}
    }

    fn convert_error(e: impl Fail) -> failure::Error {
        let e: failure::Error = e.into();
        e.context("json (s|d)erialization failure").into()
    }
}
impl Transform for JSONTransform {
    type TXState = JTXState;
    type RXState = JRXState;
    type Wire = Vec<u8>;
   
    fn tx_begin(&self, method: &'static str) -> Result<JTXState> {
        Ok(JTXState{method: method, params: json!({})})
    }

    fn tx_add_param(&self, name: &'static str, value: impl Serialize, state: &mut JTXState) -> Result<()> {
        state.params.as_object_mut().unwrap().insert(name.to_string(), serde_json::to_value(value)?);
        Ok(())
    }

    fn tx_finalize(&self, state: &mut JTXState) -> Result<Vec<u8>> {
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "method": state.method,
            "params": state.params,
            "id": format!("{}", Uuid::new_v4())
        })).map_err(Self::convert_error)
    }

    fn rx_begin(&self, data: Vec<u8>) -> Result<(String, JRXState)> {
        let value: Value = serde_json::from_slice(&data)?;
        let method = value.get("method")
            .ok_or(RPCError::UnexpectedInput{detail: "json is not expected object".to_string()})?
            .to_string();
        Ok((method, JRXState{json: value}))
    }
    
    fn rx_read_param<T>(&self, name: &'static str, state: &mut JRXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de> {
        
        let param_val = state.json.get("params")
            .ok_or(RPCError::UnexpectedInput{detail: "json is not expected object".to_string()})?
            .get(name)
            .ok_or(RPCError::UnexpectedInput{detail:
                                             format!("parameters do not contain {}", name)})?;
        return serde_json::from_value(param_val.clone()).map_err(Self::convert_error);
    }

    fn from_wire<'a, T>(&self, data: &'a Vec<u8>) -> Result<T> where
        T: Deserialize<'a>
    {
        serde_json::from_slice(data).map_err(Self::convert_error)

    }

    fn to_wire(&self, value: impl Serialize) -> Result<Self::Wire> {
        serde_json::to_vec(&value).map_err(Self::convert_error)
    }
    
}
