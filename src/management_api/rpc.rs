use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
    pub id: Option<Value>,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Value,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Deserialize)]
pub struct RoutesParams {
    pub routes: HashMap<String, crate::config::BackendConfig>,
    #[serde(default)]
    pub default: Option<crate::config::BackendConfig>,
}

#[derive(Deserialize)]
pub struct MergeRoutesParams {
    pub routes: HashMap<String, crate::config::BackendConfig>,
}