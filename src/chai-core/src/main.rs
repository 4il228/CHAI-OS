mod llm;
mod nix;
mod rpc;
mod server;

use llm::client::Client;
use llm::config::LlmConfig;
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("chai-core starting up");

    let llm_client = match LlmConfig::load() {
        Ok(cfg) => {
            let client = Arc::new(Client::new(cfg));
            tracing::info!("LLM client initialized");
            Some(client)
        }
        Err(e) => {
            tracing::warn!("LLM config not loaded ({}); LLM features disabled", e);
            None
        }
    };

    let state = Arc::new(server::State { llm_client });
    let socket_path = std::path::Path::new("/run/chai-core/chai.sock");

    tokio::select! {
        _ = server::start(socket_path, state) => {
            tracing::error!("Server exited unexpectedly");
        }
        _ = shutdown_signal() => {
            tracing::info!("Shutdown signal received");
        }
    }

    tracing::info!("chai-core shutting down gracefully");
}

async fn shutdown_signal() {
    let ctrl_c = signal::ctrl_c();

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("SIGINT received");
        }
        _ = terminate => {
            tracing::info!("SIGTERM received");
        }
    }
}
