use anyhow::{bail, Result};
use sqlx::{FromRow, SqlitePool};
use usmf_core::{Component, ComponentStats, ComponentType};

fn component_type_to_str(t: ComponentType) -> &'static str {
    match t {
        ComponentType::Weapon => "weapon",
        ComponentType::Engine => "engine",
        ComponentType::Power => "power",
        ComponentType::Sensor => "sensor",
        ComponentType::Armor => "armor",
        ComponentType::Comms => "comms",
        ComponentType::Logistics => "logistics",
    }
}

fn component_type_from_str(s: &str) -> Result<ComponentType> {
    Ok(match s {
        "weapon" => ComponentType::Weapon,
        "engine" => ComponentType::Engine,
        "power" => ComponentType::Power,
        "sensor" => ComponentType::Sensor,
        "armor" => ComponentType::Armor,
        "comms" => ComponentType::Comms,
        "logistics" => ComponentType::Logistics,
        other => bail!("unknown component_type '{other}' in database"),
    })
}

#[derive(FromRow)]
struct ComponentRow {
    id: i64,
    name: String,
    component_type: String,
    stats: String,
}

impl ComponentRow {
    fn into_domain(self) -> Result<Component> {
        Ok(Component {
            id: self.id,
            name: self.name,
            component_type: component_type_from_str(&self.component_type)?,
            stats: serde_json::from_str(&self.stats)?,
        })
    }
}

pub struct ComponentRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ComponentRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Component>> {
        let rows: Vec<ComponentRow> =
            sqlx::query_as("SELECT id, name, component_type, stats FROM components ORDER BY id")
                .fetch_all(self.pool)
                .await?;
        rows.into_iter().map(ComponentRow::into_domain).collect()
    }

    pub async fn get(&self, id: i64) -> Result<Option<Component>> {
        let row: Option<ComponentRow> =
            sqlx::query_as("SELECT id, name, component_type, stats FROM components WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        row.map(ComponentRow::into_domain).transpose()
    }

    pub async fn create(
        &self,
        name: &str,
        component_type: ComponentType,
        stats: &ComponentStats,
    ) -> Result<i64> {
        let stats_json = serde_json::to_string(stats)?;
        let id =
            sqlx::query("INSERT INTO components (name, component_type, stats) VALUES (?, ?, ?)")
                .bind(name)
                .bind(component_type_to_str(component_type))
                .bind(stats_json)
                .execute(self.pool)
                .await?
                .last_insert_rowid();
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    async fn test_pool() -> SqlitePool {
        let pool = crate::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips_stats() {
        let pool = test_pool().await;
        let repo = ComponentRepo::new(&pool);

        let stats = ComponentStats {
            weight: 300.0,
            space: 3.0,
            power_gen: 400.0,
            capabilities: HashMap::from([("indirect_fire".to_string(), 1)]),
            ..Default::default()
        };
        let id = repo
            .create("Compact Diesel Engine", ComponentType::Engine, &stats)
            .await
            .unwrap();

        let fetched = repo.get(id).await.unwrap().expect("component exists");
        assert_eq!(fetched.name, "Compact Diesel Engine");
        assert_eq!(fetched.component_type, ComponentType::Engine);
        assert_eq!(fetched.stats.weight, 300.0);
        assert_eq!(fetched.stats.capabilities.get("indirect_fire"), Some(&1));
    }

    #[tokio::test]
    async fn list_returns_all_components_in_id_order() {
        let pool = test_pool().await;
        let repo = ComponentRepo::new(&pool);
        repo.create("A", ComponentType::Weapon, &ComponentStats::default())
            .await
            .unwrap();
        repo.create("B", ComponentType::Sensor, &ComponentStats::default())
            .await
            .unwrap();

        let all = repo.list().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "A");
        assert_eq!(all[1].name, "B");
    }
}
