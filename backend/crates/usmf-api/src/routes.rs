use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use usmf_core::{ComponentStats, ComponentType};
use usmf_db::ComponentRepo;

use crate::state::AppState;

pub async fn health() -> &'static str {
    "ok"
}

pub async fn list_components(State(state): State<AppState>) -> impl IntoResponse {
    let repo = ComponentRepo::new(&state.pool);
    match repo.list().await {
        Ok(components) => Json(components).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to list components");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_component(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let repo = ComponentRepo::new(&state.pool);
    match repo.get(id).await {
        Ok(Some(component)) => Json(component).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to fetch component");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub component_type: ComponentType,
    #[serde(default)]
    pub stats: ComponentStats,
}

pub async fn create_component(
    State(state): State<AppState>,
    Json(body): Json<CreateComponentRequest>,
) -> impl IntoResponse {
    let repo = ComponentRepo::new(&state.pool);
    match repo
        .create(&body.name, body.component_type, &body.stats)
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to create component");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
