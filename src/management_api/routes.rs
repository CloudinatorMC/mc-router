use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;

use super::rpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcError, RoutesParams, MergeRoutesParams};

pub async fn handle_request(data: &str, state: Arc<RwLock<crate::AppState>>) -> JsonRpcResponse {
    let request: Result<JsonRpcRequest, _> = serde_json::from_str(data);
    match request {
        Ok(req) => {
            if req.jsonrpc != "2.0" {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid Request".to_string(),
                    }),
                    id: req.id.unwrap_or(Value::Null),
                };
            }
            let id = req.id.clone().unwrap_or(Value::Null);
            match req.method.as_str() {
                "ping" => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(Value::String("pong".to_string())),
                    error: None,
                    id,
                },
                "get_routes" => {
                    let cfg = state.read().await;
                    let routes = serde_json::to_value(&cfg.config.routes).unwrap_or(Value::Null);
                    let default = serde_json::to_value(&cfg.config.default).unwrap_or(Value::Null);
                    let result = serde_json::json!({
                        "routes": routes,
                        "default": default
                    });
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(result),
                        error: None,
                        id,
                    }
                },
                "replace_routes" => {
                    match serde_json::from_value::<RoutesParams>(req.params) {
                        Ok(params) => {
                            let mut cfg = state.write().await;
                            cfg.config.routes = params.routes;
                            cfg.config.default = params.default;
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                result: Some(Value::String("ok".to_string())),
                                error: None,
                                id,
                            }
                        },
                        Err(_) => JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32602,
                                message: "Invalid params".to_string(),
                            }),
                            id,
                        },
                    }
                },
                "merge_routes" => {
                    match serde_json::from_value::<MergeRoutesParams>(req.params) {
                        Ok(params) => {
                            let mut cfg = state.write().await;
                            for (k, v) in params.routes {
                                cfg.config.routes.insert(k.to_ascii_lowercase(), v);
                            }
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                result: Some(Value::String("ok".to_string())),
                                error: None,
                                id,
                            }
                        },
                        Err(_) => JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32602,
                                message: "Invalid params".to_string(),
                            }),
                            id,
                        },
                    }
                },
                "clear_routes" => {
                    let mut cfg = state.write().await;
                    cfg.config.routes.clear();
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(Value::String("ok".to_string())),
                        error: None,
                        id,
                    }
                },
                _ => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: "Method not found".to_string(),
                    }),
                    id,
                },
            }
        },
        Err(_) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32700,
                message: "Parse error".to_string(),
            }),
            id: Value::Null,
        },
    }
}
