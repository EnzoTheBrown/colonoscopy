use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

// ─────────────────────────────────────────────────────────────
// Domain types
// ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
enum StatusColor {
    Red,
    Orange,
    Green,
}

#[derive(Serialize, Clone)]
struct ServiceStatus {
    name: String,
    status: StatusColor,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    subservices: Vec<ServiceStatus>,
}

// ─────────────────────────────────────────────────────────────
// Application state
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    health_tree: Arc<RwLock<ServiceStatus>>,
}

// ─────────────────────────────────────────────────────────────
// HTTP handlers
// ─────────────────────────────────────────────────────────────

/// GET /health → 200 JSON { global + subservice health }
async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    // Reader lock keeps the endpoint fast even under heavy load
    let tree = state.health_tree.read().await;
    (StatusCode::OK, Json(tree.clone()))
}

// ─────────────────────────────────────────────────────────────
// Example helpers – replace with real checks in production
// ─────────────────────────────────────────────────────────────

/// Build an example health tree with two sub-services
fn initial_health() -> ServiceStatus {
    ServiceStatus {
        name: "medic".to_owned(),
        status: StatusColor::Green,
        description: Some("All systems nominal".into()),
        subservices: vec![
            ServiceStatus {
                name: "database".into(),
                status: StatusColor::Green,
                description: None,
                subservices: vec![],
            },
            ServiceStatus {
                name: "external-api".into(),
                status: StatusColor::Orange,
                description: Some("latency high".into()),
                subservices: vec![ServiceStatus {
                    name: "auth".into(),
                    status: StatusColor::Red,
                    description: Some("token refresh failed".into()),
                    subservices: vec![],
                }],
            },
        ],
    }
}

// ─────────────────────────────────────────────────────────────
// main()
// ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logging
    tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish(),
    )?;

    // Shared in-memory health tree
    let health_tree = Arc::new(RwLock::new(initial_health()));
    let state = AppState { health_tree };

    // Build router
    let app = Router::new()
        .route("/health", get(get_health))
        .with_state(state);

    // Serve
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    info!("Listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
