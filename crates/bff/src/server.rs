//! `GET /api/instances/{id}/server`: an instance's `server@<id>.ini` read
//! through the manager for a signed-in user, with the defaults, limits and
//! interfaces the web UI's Server form shows.

use crate::session::{error, Sessions};
use crate::Connect;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pai_core::client::{ClientError, ManagerFile};
use pai_core::server_cfg::{self, ServerConfig};
use pai_core::{checks, net_ifaces, paths};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// The answer to `GET /api/instances/{id}/server`, or `{"key", "args": []}`:
/// 401 with no session (the manager not asked), never the manager's detail.
pub(crate) async fn read(
    connect: Connect,
    timeout: Duration,
    sessions: Arc<Sessions>,
    id: String,
    headers: HeaderMap,
) -> Response {
    if sessions.current(&headers).is_none() {
        return error(StatusCode::UNAUTHORIZED, "session-required", &[]);
    }
    let name = format!("server@{id}.ini");
    let ask = tokio::task::spawn_blocking(move || connect()?.read_file(&name));
    let (status, key) = match tokio::time::timeout(timeout, ask).await {
        Ok(Ok(Ok(Some(file)))) => return axum::Json(document(&file)).into_response(),
        Ok(Ok(Ok(None)))
        | Ok(Ok(Err(
            ClientError::NoInstance | ClientError::BadFileName | ClientError::InvalidId,
        ))) => (StatusCode::NOT_FOUND, "client-no-instance"),
        Ok(Ok(Err(ClientError::Unavailable))) | Err(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "client-unavailable")
        }
        Ok(Ok(Err(ClientError::NotAuthorized))) => (StatusCode::FORBIDDEN, "client-not-authorized"),
        Ok(Ok(Err(_))) | Ok(Err(_)) => (StatusCode::BAD_GATEWAY, "client-other"),
    };
    error(status, key, &[])
}

/// The 200 document of the file. `values.OpencodeApiKey` is `null` and the
/// document carries `"withheld": ["OpencodeApiKey"]` exactly when the
/// manager's `withheld` flag is set, even when the text it sent still names
/// a key: every signed-in session is a member of the administrators' group
/// (sign-in refuses anyone else 403), so the bff keeps nothing on its own.
fn document(file: &ManagerFile) -> Value {
    let text = file.text.as_str();
    let mut cfg = server_cfg::from_text(text);
    if file.withheld {
        cfg.opencode_api_key = None;
    }
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let range = |key: &str| checks::server_int_range(key).map(|r| json!([r.start(), r.end()]));
    let base_url = ServerConfig {
        opencode_base_url: None,
        ..cfg.clone()
    }
    .opencode_base_url_or_default();
    let interfaces: Vec<Value> = net_ifaces::interfaces()
        .into_iter()
        .map(|i| json!({ "value": i.value, "label": i.label }))
        .collect();
    let mut doc = json!({
        "revision": revision(text),
        "values": {
            "Port": cfg.port,
            "Hostname": cfg.hostname,
            "Threads": cfg.threads,
            "ThreadsBatch": cfg.threads_batch,
            "ModelsMax": cfg.models_max,
            "ModelsMemoryMargin": cfg.models_memory_margin,
            "ModelsDir": cfg.models_dir,
            "RocblasUseHipblaslt": cfg.rocblas_use_hipblaslt.map(|on| if on { "on" } else { "off" }),
            "CudaAllreduce": cfg.cuda_allreduce,
            "CudaDisableGraphs": cfg.cuda_disable_graphs,
            "AttnRotDisable": cfg.attn_rot_disable,
            "RocprofAttach": cfg.rocprof_attach,
            "Webui": cfg.webui,
            "WebuiMcpProxy": cfg.webui_mcp_proxy,
            "LogVerbosity": cfg.log_verbosity,
            "SaveStateOnShutdown": cfg.save_state_on_shutdown,
            "StateDir": cfg.state_dir,
            "OpencodeBaseUrl": cfg.opencode_base_url,
            "OpencodeApiKey": cfg.opencode_api_key,
        },
        "defaults": {
            "Port": server_cfg::DEFAULT_PORT,
            "ModelsMax": server_cfg::DEFAULT_MODELS_MAX,
            "ModelsMemoryMargin": server_cfg::DEFAULT_MODELS_MEMORY_MARGIN,
            "OpencodeBaseUrl": base_url,
            "ModelsDir": paths::instance_models_dir().to_string_lossy(),
            "StateDir": paths::instance_state_dir().to_string_lossy(),
        },
        "limits": {
            "Port": range("Port"),
            "ModelsMax": range("ModelsMax"),
            "ModelsMemoryMargin": range("ModelsMemoryMargin"),
            "Threads": [1, cpus],
            "ThreadsBatch": [1, cpus],
        },
        "interfaces": interfaces,
    });
    if file.withheld {
        doc["withheld"] = json!(["OpencodeApiKey"]);
    }
    doc
}

/// The file's revision: the 64-bit FNV-1a hash of its text in hex, the same
/// for the same text across runs and builds.
fn revision(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}
