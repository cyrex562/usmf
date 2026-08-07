use anyhow::Result;
use sqlx::{FromRow, SqlitePool};
use usmf_core::{PersonnelLoadoutItem, PersonnelType};

#[derive(FromRow)]
struct PersonnelTypeRow {
    id: i64,
    name: String,
    role_category: Option<String>,
    max_carry_weight: f64,
    max_carry_space: f64,
    base_cost: f64,
}

#[derive(FromRow)]
struct PersonnelLoadoutRow {
    component_id: i64,
    quantity: i64,
}

pub struct PersonnelTypeRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PersonnelTypeRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    async fn load_loadout(&self, personnel_type_id: i64) -> Result<Vec<PersonnelLoadoutItem>> {
        let rows: Vec<PersonnelLoadoutRow> = sqlx::query_as(
            "SELECT component_id, quantity FROM personnel_loadout WHERE personnel_type_id = ? ORDER BY component_id",
        )
        .bind(personnel_type_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PersonnelLoadoutItem {
                component_id: r.component_id,
                quantity: r.quantity as u32,
            })
            .collect())
    }

    pub async fn list(&self) -> Result<Vec<PersonnelType>> {
        let rows: Vec<PersonnelTypeRow> = sqlx::query_as(
            "SELECT id, name, role_category, max_carry_weight, max_carry_space, base_cost FROM personnel_types ORDER BY id",
        )
        .fetch_all(self.pool)
        .await?;
        let mut personnel_types = Vec::with_capacity(rows.len());
        for row in rows {
            let loadout = self.load_loadout(row.id).await?;
            personnel_types.push(PersonnelType {
                id: row.id,
                name: row.name,
                role_category: row.role_category,
                max_carry_weight: row.max_carry_weight,
                max_carry_space: row.max_carry_space,
                base_cost: row.base_cost,
                loadout,
            });
        }
        Ok(personnel_types)
    }

    pub async fn get(&self, id: i64) -> Result<Option<PersonnelType>> {
        let row: Option<PersonnelTypeRow> = sqlx::query_as(
            "SELECT id, name, role_category, max_carry_weight, max_carry_space, base_cost FROM personnel_types WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let loadout = self.load_loadout(row.id).await?;
        Ok(Some(PersonnelType {
            id: row.id,
            name: row.name,
            role_category: row.role_category,
            max_carry_weight: row.max_carry_weight,
            max_carry_space: row.max_carry_space,
            base_cost: row.base_cost,
            loadout,
        }))
    }

    pub async fn create(
        &self,
        name: &str,
        role_category: Option<&str>,
        max_carry_weight: f64,
        max_carry_space: f64,
        base_cost: f64,
        loadout: &[PersonnelLoadoutItem],
    ) -> Result<i64> {
        let id = sqlx::query(
            "INSERT INTO personnel_types (name, role_category, max_carry_weight, max_carry_space, base_cost) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(role_category)
        .bind(max_carry_weight)
        .bind(max_carry_space)
        .bind(base_cost)
        .execute(self.pool)
        .await?
        .last_insert_rowid();

        for item in loadout {
            sqlx::query(
                "INSERT INTO personnel_loadout (personnel_type_id, component_id, quantity) VALUES (?, ?, ?)",
            )
            .bind(id)
            .bind(item.component_id)
            .bind(item.quantity as i64)
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
    async fn create_and_fetch_round_trips_loadout() {
        let pool = test_pool().await;
        let component_repo = ComponentRepo::new(&pool);
        let personnel_repo = PersonnelTypeRepo::new(&pool);

        let rifle_id = component_repo
            .create(
                "M4 Carbine",
                ComponentType::Weapon,
                &ComponentStats::default(),
            )
            .await
            .unwrap();

        let id = personnel_repo
            .create(
                "Rifleman",
                Some("Infantry"),
                60.0,
                10.0,
                0.0,
                &[PersonnelLoadoutItem {
                    component_id: rifle_id,
                    quantity: 1,
                }],
            )
            .await
            .unwrap();

        let fetched = personnel_repo.get(id).await.unwrap().expect("exists");
        assert_eq!(fetched.name, "Rifleman");
        assert_eq!(fetched.role_category.as_deref(), Some("Infantry"));
        assert_eq!(fetched.loadout.len(), 1);
    }

    #[tokio::test]
    async fn list_returns_all_personnel_types() {
        let pool = test_pool().await;
        let personnel_repo = PersonnelTypeRepo::new(&pool);

        personnel_repo
            .create("Rifleman", None, 60.0, 10.0, 0.0, &[])
            .await
            .unwrap();
        personnel_repo
            .create("Squad Leader", None, 55.0, 10.0, 0.0, &[])
            .await
            .unwrap();

        let all = personnel_repo.list().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Rifleman");
        assert_eq!(all[1].name, "Squad Leader");
    }

    #[tokio::test]
    async fn get_missing_personnel_type_returns_none() {
        let pool = test_pool().await;
        let personnel_repo = PersonnelTypeRepo::new(&pool);
        assert!(personnel_repo.get(999).await.unwrap().is_none());
    }
}
