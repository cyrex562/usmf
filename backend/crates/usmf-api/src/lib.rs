pub mod routes;
pub mod state;
#[cfg(feature = "serve-frontend")]
pub mod static_files;

use axum::routing::{get, post};
use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;

use state::AppState;

/// Builds the full router against an already-migrated pool. Split out from
/// `main` so integration tests can exercise real HTTP requests end-to-end
/// (`tower::ServiceExt::oneshot`) against an in-memory SQLite DB without
/// spinning up a TCP listener.
pub fn app(pool: SqlitePool) -> Router {
    let state = AppState { pool };

    let router = Router::new()
        .route("/health", get(routes::health))
        .route(
            "/api/components",
            get(routes::list_components).post(routes::create_component),
        )
        .route("/api/components/{id}", get(routes::get_component))
        .route(
            "/api/chassis-specs",
            get(routes::list_chassis_specs).post(routes::create_chassis_spec),
        )
        .route(
            "/api/assets",
            get(routes::list_assets).post(routes::create_asset),
        )
        .route("/api/assets/{id}", get(routes::get_asset))
        .route("/api/assets/validate", post(routes::validate_asset_draft))
        .route(
            "/api/personnel-types",
            get(routes::list_personnel_types).post(routes::create_personnel_type),
        )
        .route("/api/personnel-types/{id}", get(routes::get_personnel_type))
        .route(
            "/api/personnel-types/validate",
            post(routes::validate_personnel_type_draft),
        )
        .route(
            "/api/units",
            get(routes::list_units).post(routes::create_unit),
        )
        .route(
            "/api/units/{id}",
            get(routes::get_unit).put(routes::update_unit),
        )
        .route("/api/units/{id}/rollup", get(routes::get_unit_rollup))
        .route(
            "/api/units/{id}/relationships",
            get(routes::list_relationships_for_unit),
        )
        .route(
            "/api/relationships",
            get(routes::list_relationships).post(routes::create_relationship),
        )
        .route(
            "/api/relationships/{id}/detach",
            post(routes::detach_relationship),
        )
        .route(
            "/api/relationship-types",
            get(routes::list_relationship_types).post(routes::create_relationship_type),
        );

    // Any route above wins; everything else falls through to the embedded
    // SPA (release builds only -- see static_files.rs).
    #[cfg(feature = "serve-frontend")]
    let router = router.fallback(static_files::static_handler);

    router.layer(CorsLayer::permissive()).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn test_app() -> Router {
        let pool = usmf_db::connect("sqlite::memory:").await.unwrap();
        usmf_db::run_migrations(&pool).await.unwrap();
        app(pool)
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn post_json(app: &Router, path: &str, body: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "POST {path} failed: {}",
            response.status()
        );
        body_json(response).await
    }

    async fn get_json(app: &Router, path: &str) -> Value {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "GET {path} failed: {}",
            response.status()
        );
        body_json(response).await
    }

    /// Exercises the real wiring behind the Commander's Dashboard end to end
    /// over HTTP: a unit with its own asset, organically commanding a second
    /// unit, and OPCON'ing in a third from elsewhere. Mirrors
    /// design_doc.md §2.1's doctrinal-effects table -- OPCON counts toward
    /// combat power but not sustainment -- and the `scope=organic` vs.
    /// default `effective` split from this issue.
    #[tokio::test]
    async fn unit_rollup_wires_real_relationships_through_scope_and_sustainment_rules() {
        let app = test_app().await;

        let component = post_json(
            &app,
            "/api/components",
            json!({
                "name": "Fuel-hungry Engine",
                "component_type": "engine",
                "stats": { "weight": 500, "space": 2, "power_draw": 50 }
            }),
        )
        .await;
        let component_id = component["id"].as_i64().unwrap();

        let asset_b = post_json(
            &app,
            "/api/assets",
            json!({
                "name": "B Vehicle",
                "chassis_type": "Light Wheeled",
                "components": [{ "component_id": component_id, "quantity": 1 }]
            }),
        )
        .await;
        let asset_c = post_json(
            &app,
            "/api/assets",
            json!({
                "name": "C Vehicle",
                "chassis_type": "Light Wheeled",
                "components": [{ "component_id": component_id, "quantity": 1 }]
            }),
        )
        .await;

        let unit_a = post_json(
            &app,
            "/api/units",
            json!({ "name": "A HQ", "unit_type": "hq" }),
        )
        .await;
        let a = unit_a["id"].as_i64().unwrap();

        let unit_b = post_json(
            &app,
            "/api/units",
            json!({
                "name": "B",
                "unit_type": "line",
                "own_assets": [{ "asset_id": asset_b["id"], "quantity": 1 }]
            }),
        )
        .await;
        let b = unit_b["id"].as_i64().unwrap();

        let unit_c = post_json(
            &app,
            "/api/units",
            json!({
                "name": "C",
                "unit_type": "line",
                "own_assets": [{ "asset_id": asset_c["id"], "quantity": 1 }]
            }),
        )
        .await;
        let c = unit_c["id"].as_i64().unwrap();

        post_json(
            &app,
            "/api/relationships",
            json!({ "superior_unit_id": a, "subordinate_unit_id": b, "relationship_type": "Organic" }),
        )
        .await;
        post_json(
            &app,
            "/api/relationships",
            json!({ "superior_unit_id": a, "subordinate_unit_id": c, "relationship_type": "OPCON" }),
        )
        .await;

        let effective = get_json(&app, &format!("/api/units/{a}/rollup")).await;
        assert_eq!(
            effective["weight"], 1000.0,
            "effective scope should include both B's and C's weight"
        );
        assert_eq!(
            effective["daily_supply_consumption"], 50.0,
            "OPCON's sustainment shouldn't transfer to A -- only B's organic draw should"
        );

        let organic = get_json(&app, &format!("/api/units/{a}/rollup?scope=organic")).await;
        assert_eq!(
            organic["weight"], 500.0,
            "organic-only scope should exclude C (OPCON, not Organic) entirely"
        );
        assert_eq!(organic["daily_supply_consumption"], 50.0);
    }

    #[tokio::test]
    async fn unit_rollup_404s_for_missing_unit() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/units/999/rollup")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// Issue #30: a custom relationship type beyond the seeded six is
    /// creatable over HTTP, shows up in the list, and is immediately usable
    /// by `POST /api/relationships`.
    #[tokio::test]
    async fn create_relationship_type_adds_a_custom_type() {
        let app = test_app().await;

        let created = post_json(
            &app,
            "/api/relationship-types",
            json!({
                "name": "Liaison",
                "rules": {
                    "includes_in_span_of_control": false,
                    "sustainment_transfers": false,
                    "includes_in_combat_power_rollup": false
                }
            }),
        )
        .await;
        assert_eq!(created["name"], "Liaison");

        let types = get_json(&app, "/api/relationship-types").await;
        let names: Vec<&str> = types
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"Liaison"));

        let hq = post_json(
            &app,
            "/api/units",
            json!({ "name": "Division HQ", "unit_type": "hq" }),
        )
        .await;
        let team = post_json(
            &app,
            "/api/units",
            json!({ "name": "Liaison Team", "unit_type": "support" }),
        )
        .await;
        post_json(
            &app,
            "/api/relationships",
            json!({
                "superior_unit_id": hq["id"],
                "subordinate_unit_id": team["id"],
                "relationship_type": "Liaison"
            }),
        )
        .await;
    }
}
