use anyhow::Result;
use sqlx::{FromRow, SqlitePool};
use usmf_core::{Asset, AssetComponent};

#[derive(FromRow)]
struct AssetRow {
    id: i64,
    name: String,
    chassis_type: String,
}

#[derive(FromRow)]
struct AssetComponentRow {
    component_id: i64,
    quantity: i64,
}

pub struct AssetRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AssetRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    async fn load_components(&self, asset_id: i64) -> Result<Vec<AssetComponent>> {
        let rows: Vec<AssetComponentRow> = sqlx::query_as(
            "SELECT component_id, quantity FROM asset_components WHERE asset_id = ? ORDER BY component_id",
        )
        .bind(asset_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AssetComponent {
                component_id: r.component_id,
                quantity: r.quantity as u32,
            })
            .collect())
    }

    pub async fn list(&self) -> Result<Vec<Asset>> {
        let rows: Vec<AssetRow> =
            sqlx::query_as("SELECT id, name, chassis_type FROM assets ORDER BY id")
                .fetch_all(self.pool)
                .await?;
        let mut assets = Vec::with_capacity(rows.len());
        for row in rows {
            let components = self.load_components(row.id).await?;
            assets.push(Asset {
                id: row.id,
                name: row.name,
                chassis_type: row.chassis_type,
                components,
            });
        }
        Ok(assets)
    }

    pub async fn get(&self, id: i64) -> Result<Option<Asset>> {
        let row: Option<AssetRow> =
            sqlx::query_as("SELECT id, name, chassis_type FROM assets WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let components = self.load_components(row.id).await?;
        Ok(Some(Asset {
            id: row.id,
            name: row.name,
            chassis_type: row.chassis_type,
            components,
        }))
    }

    pub async fn create(
        &self,
        name: &str,
        chassis_type: &str,
        components: &[AssetComponent],
    ) -> Result<i64> {
        let id = sqlx::query("INSERT INTO assets (name, chassis_type) VALUES (?, ?)")
            .bind(name)
            .bind(chassis_type)
            .execute(self.pool)
            .await?
            .last_insert_rowid();

        for component in components {
            sqlx::query(
                "INSERT INTO asset_components (asset_id, component_id, quantity) VALUES (?, ?, ?)",
            )
            .bind(id)
            .bind(component.component_id)
            .bind(component.quantity as i64)
            .execute(self.pool)
            .await?;
        }

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::ComponentRepo;
    use usmf_core::{ComponentStats, ComponentType};

    async fn test_pool() -> SqlitePool {
        let pool = crate::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips_components() {
        let pool = test_pool().await;
        let component_repo = ComponentRepo::new(&pool);
        let asset_repo = AssetRepo::new(&pool);

        let engine_id = component_repo
            .create(
                "Diesel Engine",
                ComponentType::Engine,
                &ComponentStats::default(),
            )
            .await
            .unwrap();
        let gun_id = component_repo
            .create(
                "120mm Cannon",
                ComponentType::Weapon,
                &ComponentStats::default(),
            )
            .await
            .unwrap();

        let id = asset_repo
            .create(
                "M1A5 Tank",
                "Heavy Tracked",
                &[
                    AssetComponent {
                        component_id: engine_id,
                        quantity: 1,
                    },
                    AssetComponent {
                        component_id: gun_id,
                        quantity: 1,
                    },
                ],
            )
            .await
            .unwrap();

        let fetched = asset_repo.get(id).await.unwrap().expect("asset exists");
        assert_eq!(fetched.name, "M1A5 Tank");
        assert_eq!(fetched.chassis_type, "Heavy Tracked");
        assert_eq!(fetched.components.len(), 2);
    }

    #[tokio::test]
    async fn list_returns_all_assets_with_their_components() {
        let pool = test_pool().await;
        let asset_repo = AssetRepo::new(&pool);

        asset_repo.create("A", "Light Wheeled", &[]).await.unwrap();
        asset_repo.create("B", "Light Wheeled", &[]).await.unwrap();

        let all = asset_repo.list().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "A");
        assert_eq!(all[1].name, "B");
    }

    #[tokio::test]
    async fn get_missing_asset_returns_none() {
        let pool = test_pool().await;
        let asset_repo = AssetRepo::new(&pool);
        assert!(asset_repo.get(999).await.unwrap().is_none());
    }
}
