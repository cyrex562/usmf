use anyhow::Result;
use sqlx::{FromRow, SqlitePool};
use usmf_core::RulesetSpec;

#[derive(FromRow)]
struct RulesetRow {
    id: String,
    display_name: String,
    source: Option<String>,
    supports_individual: bool,
    supports_aggregate: bool,
}

impl From<RulesetRow> for RulesetSpec {
    fn from(row: RulesetRow) -> Self {
        RulesetSpec {
            id: row.id,
            display_name: row.display_name,
            source: row.source,
            supports_individual: row.supports_individual,
            supports_aggregate: row.supports_aggregate,
        }
    }
}

pub struct RulesetRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RulesetRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<RulesetSpec>> {
        let rows: Vec<RulesetRow> = sqlx::query_as(
            "SELECT id, display_name, source, supports_individual, supports_aggregate
             FROM rulesets ORDER BY id",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<RulesetSpec>> {
        let row: Option<RulesetRow> = sqlx::query_as(
            "SELECT id, display_name, source, supports_individual, supports_aggregate
             FROM rulesets WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn create(&self, spec: &RulesetSpec) -> Result<()> {
        sqlx::query(
            "INSERT INTO rulesets (id, display_name, source, supports_individual, supports_aggregate)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&spec.id)
        .bind(&spec.display_name)
        .bind(&spec.source)
        .bind(spec.supports_individual)
        .bind(spec.supports_aggregate)
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
    async fn seed_data_has_legacy_linear_v1() {
        let pool = test_pool().await;
        let repo = RulesetRepo::new(&pool);

        let all = repo.list().await.unwrap();
        assert_eq!(all.len(), 1);
        let legacy = &all[0];
        assert_eq!(legacy.id, "legacy_linear_v1");
        assert!(legacy.supports_individual);
        assert!(legacy.supports_aggregate);
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips() {
        let pool = test_pool().await;
        let repo = RulesetRepo::new(&pool);

        let spec = RulesetSpec {
            id: "aggregate_strength_v1".to_string(),
            display_name: "Aggregate Strength (CRT)".to_string(),
            source: Some("usmf-sim built-in".to_string()),
            supports_individual: false,
            supports_aggregate: true,
        };
        repo.create(&spec).await.unwrap();

        let fetched = repo
            .get("aggregate_strength_v1")
            .await
            .unwrap()
            .expect("exists");
        assert_eq!(fetched.display_name, "Aggregate Strength (CRT)");
        assert!(!fetched.supports_individual);
        assert!(fetched.supports_aggregate);
        assert!(repo.get("does_not_exist").await.unwrap().is_none());
    }
}
