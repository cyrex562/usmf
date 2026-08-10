mod assets;
mod chassis_specs;
mod components;
mod personnel_types;
mod rulesets;
mod unit_relationships;
mod units;

pub use assets::AssetRepo;
pub use chassis_specs::ChassisSpecRepo;
pub use components::ComponentRepo;
pub use personnel_types::PersonnelTypeRepo;
pub use rulesets::RulesetRepo;
pub use unit_relationships::{CreateRelationshipError, UnitRelationshipRepo};
pub use units::UnitRepo;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connects_and_migrates_in_memory_db() {
        let pool = connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM components")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 0);
    }
}
