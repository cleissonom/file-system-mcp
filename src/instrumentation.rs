use crate::observer::{Observer, Outcome};
use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::ServerConfig;

pub fn process(
    config: &ServerConfig,
    observer: Option<&Observer>,
    request: JsonRpcRequest,
    request_bytes: u64,
) -> Option<JsonRpcResponse> {
    let Some(observer) = observer.filter(|_| request.method == "tools/call") else {
        return crate::process_request(config, request);
    };
    let name = request
        .params
        .as_ref()
        .and_then(|params| params.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or("unknown_tool");
    let token = observer.begin(name, request_bytes);
    let response = crate::process_request(config, request);
    let response_bytes = response
        .as_ref()
        .and_then(|value| serde_json::to_vec(value).ok())
        .map_or(0, |bytes| bytes.len() as u64);
    observer.finish(token, outcome(response.as_ref()), response_bytes);
    response
}

fn outcome(response: Option<&JsonRpcResponse>) -> Outcome {
    let Some(response) = response.filter(|response| response.error.is_none()) else {
        return Outcome::ProtocolError;
    };
    if response
        .result
        .as_ref()
        .and_then(|result| result.get("isError"))
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        Outcome::ToolError
    } else {
        Outcome::Success
    }
}
