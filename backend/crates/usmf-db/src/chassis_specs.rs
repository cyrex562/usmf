use anyhow::Result;
use sqlx::{FromRow, SqlitePool};
use usmf_core::ChassisSpec;

#[derive(FromRow)]
struct ChassisSpecRow {
    name: String,
    max_weight: f64,
    max_space: f64,
    base_cost: f64,
}

impl From<ChassisSpecRow> for ChassisSpec {
    fn from(row: ChassisSpecRow) -> Self {
        ChassisSpec {
            name: row.name,
            max_weight: row.max_weight,
            max_space: row.max_space,
            base_cost: row.base_cost,
        }
    }
}

pub struct ChassisSpecRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ChassisSpecRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<ChassisSpec>> {
        let rows: Vec<ChassisSpecRow> = sqlx::query_as(
            "SELECT name, max_weight, max_space, base_cost FROM chassis_specs ORDER BY name",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get(&self, name: &str) -> Result<Option<ChassisSpec>> {
        let row: Option<ChassisSpecRow> = sqlx::query_as(
            "SELECT name, max_weight, max_space, base_cost FROM chassis_specs WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn create(&self, spec: &ChassisSpec) -> Result<()> {
        sqlx::query(
            "INSERT INTO chassis_specs (name, max_weight, max_space, base_cost) VALUES (?, ?, ?, ?)",
        )
        .bind(&spec.name)
        .bind(spec.max_weight)
        .bind(spec.max_space)
        .bind(spec.base_cost)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = crate::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn seed_data_is_present_after_migration() {
        let pool = test_pool().await;
        let repo = ChassisSpecRepo::new(&pool);

        let all = repo.list().await.unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|c| c.name == "Heavy Tracked"));
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips() {
        let pool = test_pool().await;
        let repo = ChassisSpecRepo::new(&pool);

        let spec = ChassisSpec {
            name: "Amphibious".into(),
            max_weight: 3000.0,
            max_space: 12.0,
            base_cost: 1500.0,
        };
        repo.create(&spec).await.unwrap();

        let fetched = repo.get("Amphibious").await.unwrap().expect("exists");
        assert_eq!(fetched.max_weight, 3000.0);
        assert!(repo.get("Nonexistent").await.unwrap().is_none());
    }
}
