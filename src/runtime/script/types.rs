//! Types for script execution and host communication.
//!
//! Contains message types for JS-to-Rust communication and response structures.

use serde::{Deserialize, Serialize};

/// Message from JS to async handler for `host.call()`
#[derive(Debug)]
pub(crate) enum HostMessage {
    Call {
        module: String,
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: Option<serde_json::Value>,
        response_tx: std::sync::mpsc::Sender<HostCallResult>,
    },
}

/// Result of a `host.call()` invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HostCallResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request body for script execution
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ScriptRequest {
    #[serde(default)]
    pub input: serde_json::Value,
}

/// Response from script execution
#[derive(Debug, Serialize)]
pub(crate) struct ScriptResponse {
    pub result: serde_json::Value,
    pub calls_executed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_host_call_result_serialization() {
        let result = HostCallResult {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: json!({"message": "ok"}),
            error: None,
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["status"], 200);
        assert_eq!(json["body"]["message"], "ok");
        assert!(json.get("error").is_none() || json["error"].is_null());
    }

    #[test]
    fn test_host_call_result_with_error() {
        let result = HostCallResult {
            status: 500,
            headers: vec![],
            body: json!(null),
            error: Some("HANDLER_ERROR".to_string()),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["status"], 500);
        assert_eq!(json["error"], "HANDLER_ERROR");
    }

    #[test]
    fn test_script_response_serialization() {
        let response = ScriptResponse {
            result: json!({"orderId": 123}),
            calls_executed: 3,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["result"]["orderId"], 123);
        assert_eq!(json["calls_executed"], 3);
    }
}
