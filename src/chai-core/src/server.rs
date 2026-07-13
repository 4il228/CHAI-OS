use crate::llm::client::Client;
use crate::nix;
use crate::rpc::JsonRpcRequest;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct State {
    pub llm_client: Option<Arc<Client>>,
}

#[cfg(unix)]
pub async fn start(socket_path: &Path, state: Arc<State>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if socket_path.exists() {
        tokio::fs::remove_file(socket_path).await?;
    }

    let listener = tokio::net::UnixListener::bind(socket_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660)).await?;
    }

    tracing::info!(target: "chai-core::server", "UDS listener started on {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = Arc::clone(&state);
                tokio::spawn(handle_connection(stream, state));
            }
            Err(e) => {
                tracing::error!(target: "chai-core::server", "Accept error: {}", e);
            }
        }
    }
}

#[cfg(not(unix))]
pub async fn start(_socket_path: &Path, _state: Arc<State>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::warn!(target: "chai-core::server",
        "Unix domain sockets not available; falling back to TCP on 127.0.0.1:9842"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9842").await?;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!(target: "chai-core::server", "TCP connection from {}", addr);
                tokio::spawn(handle_connection(stream, Arc::clone(&_state)));
            }
            Err(e) => {
                tracing::error!(target: "chai-core::server", "Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    state: Arc<State>,
) {
    let (reader, mut writer) = tokio::io::split(&mut stream);
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match buf_reader.read_line(&mut line).await {
            Ok(0) => {
                tracing::debug!(target: "chai-core::server", "Client disconnected");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let response = process_message(trimmed, &state).await;
                if let Some(resp) = response {
                    let json = serde_json::to_string(&resp).unwrap_or_else(|e| {
                        tracing::error!(target: "chai-core::server", "Serialization error: {}", e);
                        String::new()
                    });
                    if json.is_empty() {
                        break;
                    }
                    if let Err(e) = writer.write_all(json.as_bytes()).await {
                        tracing::error!(target: "chai-core::server", "Write error: {}", e);
                        break;
                    }
                    if let Err(e) = writer.write_all(b"\n").await {
                        tracing::error!(target: "chai-core::server", "Write error: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::error!(target: "chai-core::server", "Read error: {}", e);
                break;
            }
        }
    }
}

async fn process_message(body: &str, state: &State) -> Option<serde_json::Value> {
    let request: Result<JsonRpcRequest, _> = serde_json::from_str(body);

    match request {
        Ok(req) => {
            let is_notification = req.id.is_none();
            let id = req.id.unwrap_or(serde_json::Value::Null);

            let response = route_request(&req.method, &req.params, &id, state).await;

            if is_notification {
                tracing::debug!(target: "chai-core::server", "Notification processed: {}", req.method);
                None
            } else {
                Some(response)
            }
        }
        Err(e) => {
            tracing::error!(target: "chai-core::server", "Parse error: {}", e);
            let id = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(serde_json::Value::Null);
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": "Parse error"},
                "id": id,
            }))
        }
    }
}

async fn route_request(
    method: &str,
    params: &Option<serde_json::Value>,
    id: &serde_json::Value,
    state: &State,
) -> serde_json::Value {
    match method {
        "execute_intent" => {
            let prompt = params
                .as_ref()
                .and_then(|p| p.get("prompt"))
                .and_then(|p| p.as_str())
                .unwrap_or("");

            if prompt.is_empty() {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {"code": -32602, "message": "Missing 'prompt' parameter"},
                    "id": id,
                });
            }

            tracing::info!(target: "chai-core::server", "execute_intent: {}", prompt);

            let client = match &state.llm_client {
                Some(c) => c,
                None => {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {"code": -32000, "message": "LLM client not available"},
                        "id": id,
                    });
                }
            };

            let llm_result = client.generate(prompt).await;
            let llm_text = match llm_result {
                Ok(t) => t,
                Err(e) => {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {"code": -32000, "message": format!("LLM error: {e}")},
                        "id": id,
                    });
                }
            };

            let plan = match nix::parser::parse_llm_output(&llm_text) {
                Ok(p) => p,
                Err(e) => {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {"code": -32000, "message": format!("Failed to parse LLM output: {e}")},
                        "id": id,
                    });
                }
            };

            let nix_content = nix::generator::generate_nix(&plan);
            let result_path = nix::validator::validate_and_write(&nix_content, "chai-generation");

            match result_path {
                Ok(path) => {
                    let path_str = path.to_string_lossy().to_string();
                    tracing::info!(target: "chai-core::server", "Nix file generated: {}", path_str);
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": {
                            "status": "success",
                            "nix_file": path_str,
                            "content": nix_content,
                        },
                        "id": id,
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {"code": -32000, "message": format!("Nix generation error: {e}")},
                        "id": id,
                    })
                }
            }
        }
        _ => {
            tracing::warn!(target: "chai-core::server", "Method not found: {}", method);
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32601, "message": "Method not found"},
                "id": id,
            })
        }
    }
}
