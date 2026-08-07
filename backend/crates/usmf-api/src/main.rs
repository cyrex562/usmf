mod routes;
mod state;

use axum::routing::get;
use axum::Router;
use state::AppState;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://usmf.db".to_string());
    let pool = usmf_db::connect(&database_url).await?;
    usmf_db::run_migrations(&pool).await?;

    let state = AppState { pool };

    let app = Router::new()
        .route("/health", get(routes::health))
        .route(
            "/api/components",
            get(routes::list_components).post(routes::create_component),
        )
        .route("/api/components/{id}", get(routes::get_component))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("usmf-api listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
