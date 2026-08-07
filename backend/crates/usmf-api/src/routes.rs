use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use usmf_core::{
    rollup_unit, validate_asset, validate_personnel_loadout, Asset, AssetComponent, ChassisSpec,
    ComponentStats, ComponentType, FormationKind, PersonnelComposition, PersonnelLoadoutItem,
    PersonnelType, Unit, UnitAsset, UnitType,
};
use usmf_db::{AssetRepo, ChassisSpecRepo, ComponentRepo, PersonnelTypeRepo, UnitRepo};

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

pub async fn list_chassis_specs(State(state): State<AppState>) -> impl IntoResponse {
    let repo = ChassisSpecRepo::new(&state.pool);
    match repo.list().await {
        Ok(specs) => Json(specs).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to list chassis specs");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateChassisSpecRequest {
    pub name: String,
    pub max_weight: f64,
    pub max_space: f64,
    #[serde(default)]
    pub base_cost: f64,
}

pub async fn create_chassis_spec(
    State(state): State<AppState>,
    Json(body): Json<CreateChassisSpecRequest>,
) -> impl IntoResponse {
    let repo = ChassisSpecRepo::new(&state.pool);
    let spec = ChassisSpec {
        name: body.name,
        max_weight: body.max_weight,
        max_space: body.max_space,
        base_cost: body.base_cost,
    };
    match repo.create(&spec).await {
        Ok(()) => (StatusCode::CREATED, Json(spec)).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to create chassis spec");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn list_assets(State(state): State<AppState>) -> impl IntoResponse {
    let repo = AssetRepo::new(&state.pool);
    match repo.list().await {
        Ok(assets) => Json(assets).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to list assets");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_asset(State(state): State<AppState>, Path(id): Path<i64>) -> impl IntoResponse {
    let repo = AssetRepo::new(&state.pool);
    match repo.get(id).await {
        Ok(Some(asset)) => Json(asset).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to fetch asset");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateAssetRequest {
    pub name: String,
    pub chassis_type: String,
    #[serde(default)]
    pub components: Vec<AssetComponent>,
}

pub async fn create_asset(
    State(state): State<AppState>,
    Json(body): Json<CreateAssetRequest>,
) -> impl IntoResponse {
    let repo = AssetRepo::new(&state.pool);
    match repo
        .create(&body.name, &body.chassis_type, &body.components)
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to create asset");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ValidateAssetRequest {
    pub chassis_type: String,
    #[serde(default)]
    pub components: Vec<AssetComponent>,
}

/// Validates a *draft* asset (chassis + components straight from the request
/// body) without requiring it to be saved first -- this is what the Asset
/// Designer's live HUD calls on every edit.
pub async fn validate_asset_draft(
    State(state): State<AppState>,
    Json(body): Json<ValidateAssetRequest>,
) -> impl IntoResponse {
    let chassis_repo = ChassisSpecRepo::new(&state.pool);
    let component_repo = ComponentRepo::new(&state.pool);

    let chassis = match chassis_repo.get(&body.chassis_type).await {
        Ok(chassis) => chassis,
        Err(err) => {
            tracing::error!(%err, "failed to fetch chassis spec");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let components = match component_repo.list().await {
        Ok(components) => components,
        Err(err) => {
            tracing::error!(%err, "failed to list components");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let draft = Asset {
        id: 0,
        name: String::new(),
        chassis_type: body.chassis_type,
        components: body.components,
    };
    Json(validate_asset(&draft, chassis.as_ref(), &components)).into_response()
}

pub async fn list_personnel_types(State(state): State<AppState>) -> impl IntoResponse {
    let repo = PersonnelTypeRepo::new(&state.pool);
    match repo.list().await {
        Ok(personnel_types) => Json(personnel_types).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to list personnel types");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_personnel_type(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let repo = PersonnelTypeRepo::new(&state.pool);
    match repo.get(id).await {
        Ok(Some(personnel_type)) => Json(personnel_type).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to fetch personnel type");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreatePersonnelTypeRequest {
    pub name: String,
    #[serde(default)]
    pub role_category: Option<String>,
    pub max_carry_weight: f64,
    pub max_carry_space: f64,
    #[serde(default)]
    pub base_cost: f64,
    #[serde(default)]
    pub loadout: Vec<PersonnelLoadoutItem>,
}

pub async fn create_personnel_type(
    State(state): State<AppState>,
    Json(body): Json<CreatePersonnelTypeRequest>,
) -> impl IntoResponse {
    let repo = PersonnelTypeRepo::new(&state.pool);
    match repo
        .create(
            &body.name,
            body.role_category.as_deref(),
            body.max_carry_weight,
            body.max_carry_space,
            body.base_cost,
            &body.loadout,
        )
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to create personnel type");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ValidatePersonnelTypeRequest {
    #[serde(default)]
    pub role_category: Option<String>,
    pub max_carry_weight: f64,
    pub max_carry_space: f64,
    #[serde(default)]
    pub base_cost: f64,
    #[serde(default)]
    pub loadout: Vec<PersonnelLoadoutItem>,
}

/// Validates a *draft* personnel type (carry capacity + loadout straight from
/// the request body) without requiring it to be saved first -- same pattern as
/// `validate_asset_draft`, for the Personnel Designer's live HUD.
pub async fn validate_personnel_type_draft(
    State(state): State<AppState>,
    Json(body): Json<ValidatePersonnelTypeRequest>,
) -> impl IntoResponse {
    let component_repo = ComponentRepo::new(&state.pool);
    let components = match component_repo.list().await {
        Ok(components) => components,
        Err(err) => {
            tracing::error!(%err, "failed to list components");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let draft = PersonnelType {
        id: 0,
        name: String::new(),
        role_category: body.role_category,
        max_carry_weight: body.max_carry_weight,
        max_carry_space: body.max_carry_space,
        base_cost: body.base_cost,
        loadout: body.loadout,
    };
    Json(validate_personnel_loadout(&draft, &components)).into_response()
}

pub async fn list_units(State(state): State<AppState>) -> impl IntoResponse {
    let repo = UnitRepo::new(&state.pool);
    match repo.list().await {
        Ok(units) => Json(units).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to list units");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_unit(State(state): State<AppState>, Path(id): Path<i64>) -> impl IntoResponse {
    let repo = UnitRepo::new(&state.pool);
    match repo.get(id).await {
        Ok(Some(unit)) => Json(unit).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to fetch unit");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn default_formation_kind() -> FormationKind {
    FormationKind::Standing
}

#[derive(Deserialize)]
pub struct UpsertUnitRequest {
    pub name: String,
    pub unit_type: UnitType,
    #[serde(default = "default_formation_kind")]
    pub formation_kind: FormationKind,
    #[serde(default)]
    pub own_assets: Vec<UnitAsset>,
    #[serde(default)]
    pub personnel: PersonnelComposition,
    #[serde(default)]
    pub c2_capacity: Option<u32>,
}

impl UpsertUnitRequest {
    fn into_unit(self, id: i64) -> Unit {
        Unit {
            id,
            name: self.name,
            unit_type: self.unit_type,
            formation_kind: self.formation_kind,
            own_assets: self.own_assets,
            personnel: self.personnel,
            c2_capacity: self.c2_capacity,
        }
    }
}

pub async fn create_unit(
    State(state): State<AppState>,
    Json(body): Json<UpsertUnitRequest>,
) -> impl IntoResponse {
    let repo = UnitRepo::new(&state.pool);
    let unit = body.into_unit(0);
    match repo.create(&unit).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to create unit");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn update_unit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpsertUnitRequest>,
) -> impl IntoResponse {
    let repo = UnitRepo::new(&state.pool);
    let unit = body.into_unit(id);
    match repo.update(id, &unit).await {
        Ok(true) => Json(serde_json::json!({ "id": id })).into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to update unit");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Rolls up just this unit's own composition (`own_assets`/`personnel`) --
/// no relationships are wired in yet, so this doesn't walk an effective
/// command tree (that's the Commander's Dashboard issue, once
/// UnitRelationshipRepo exists). Useful today to preview a single unit's
/// weight/cost/capabilities/span-of-control before it has any subordinates.
pub async fn get_unit_rollup(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let unit_repo = UnitRepo::new(&state.pool);
    let asset_repo = AssetRepo::new(&state.pool);
    let chassis_repo = ChassisSpecRepo::new(&state.pool);
    let personnel_repo = PersonnelTypeRepo::new(&state.pool);
    let component_repo = ComponentRepo::new(&state.pool);

    let unit = match unit_repo.get(id).await {
        Ok(Some(unit)) => unit,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to fetch unit");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let components = match component_repo.list().await {
        Ok(components) => components,
        Err(err) => {
            tracing::error!(%err, "failed to list components");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut asset_totals = HashMap::new();
    for owned in &unit.own_assets {
        if asset_totals.contains_key(&owned.asset_id) {
            continue;
        }
        let asset = match asset_repo.get(owned.asset_id).await {
            Ok(Some(asset)) => asset,
            Ok(None) => continue,
            Err(err) => {
                tracing::error!(%err, "failed to fetch asset for rollup");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        let chassis = match chassis_repo.get(&asset.chassis_type).await {
            Ok(chassis) => chassis,
            Err(err) => {
                tracing::error!(%err, "failed to fetch chassis spec for rollup");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        let validation = validate_asset(&asset, chassis.as_ref(), &components);
        asset_totals.insert(owned.asset_id, validation.totals);
    }

    let mut personnel_totals = HashMap::new();
    if let PersonnelComposition::Detailed { entries } = &unit.personnel {
        for entry in entries {
            if personnel_totals.contains_key(&entry.personnel_type_id) {
                continue;
            }
            let personnel_type = match personnel_repo.get(entry.personnel_type_id).await {
                Ok(Some(pt)) => pt,
                Ok(None) => continue,
                Err(err) => {
                    tracing::error!(%err, "failed to fetch personnel type for rollup");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            };
            let validation = validate_personnel_loadout(&personnel_type, &components);
            personnel_totals.insert(entry.personnel_type_id, validation.totals);
        }
    }

    let rollup = rollup_unit(id, &[unit], &[], None, &asset_totals, &personnel_totals);
    Json(rollup).into_response()
}
