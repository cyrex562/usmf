use anyhow::{bail, Result};
use sqlx::{FromRow, SqlitePool};
use usmf_core::{FormationKind, PersonnelComposition, Unit, UnitAsset, UnitPersonnelEntry, UnitType};

fn unit_type_to_str(t: UnitType) -> &'static str {
    match t {
        UnitType::Hq => "hq",
        UnitType::Line => "line",
        UnitType::Support => "support",
    }
}

fn unit_type_from_str(s: &str) -> Result<UnitType> {
    Ok(match s {
        "hq" => UnitType::Hq,
        "line" => UnitType::Line,
        "support" => UnitType::Support,
        other => bail!("unknown unit_type '{other}' in database"),
    })
}

fn formation_kind_to_str(k: FormationKind) -> &'static str {
    match k {
        FormationKind::Standing => "standing",
        FormationKind::TaskForce => "task_force",
    }
}

fn formation_kind_from_str(s: &str) -> Result<FormationKind> {
    Ok(match s {
        "standing" => FormationKind::Standing,
        "task_force" => FormationKind::TaskForce,
        other => bail!("unknown formation_kind '{other}' in database"),
    })
}

#[derive(FromRow)]
struct UnitRow {
    id: i64,
    name: String,
    unit_type: String,
    formation_kind: String,
    c2_capacity: Option<i64>,
    personnel_mode: String,
    personnel_simplified_count: i64,
}

#[derive(FromRow)]
struct UnitAssetRow {
    asset_id: i64,
    quantity: i64,
}

#[derive(FromRow)]
struct UnitPersonnelRow {
    personnel_type_id: i64,
    quantity: i64,
}

pub struct UnitRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> UnitRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    async fn load_own_assets(&self, unit_id: i64) -> Result<Vec<UnitAsset>> {
        let rows: Vec<UnitAssetRow> = sqlx::query_as(
            "SELECT asset_id, quantity FROM unit_assets WHERE unit_id = ? ORDER BY asset_id",
        )
        .bind(unit_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnitAsset {
                asset_id: r.asset_id,
                quantity: r.quantity as u32,
            })
            .collect())
    }

    async fn load_personnel_entries(&self, unit_id: i64) -> Result<Vec<UnitPersonnelEntry>> {
        let rows: Vec<UnitPersonnelRow> = sqlx::query_as(
            "SELECT personnel_type_id, quantity FROM unit_personnel WHERE unit_id = ? ORDER BY personnel_type_id",
        )
        .bind(unit_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnitPersonnelEntry {
                personnel_type_id: r.personnel_type_id,
                quantity: r.quantity as u32,
            })
            .collect())
    }

    async fn hydrate(&self, row: UnitRow) -> Result<Unit> {
        let own_assets = self.load_own_assets(row.id).await?;
        let personnel = match row.personnel_mode.as_str() {
            "detailed" => PersonnelComposition::Detailed {
                entries: self.load_personnel_entries(row.id).await?,
            },
            _ => PersonnelComposition::Simplified {
                count: row.personnel_simplified_count as u32,
            },
        };
        Ok(Unit {
            id: row.id,
            name: row.name,
            unit_type: unit_type_from_str(&row.unit_type)?,
            formation_kind: formation_kind_from_str(&row.formation_kind)?,
            own_assets,
            personnel,
            c2_capacity: row.c2_capacity.map(|c| c as u32),
        })
    }

    pub async fn list(&self) -> Result<Vec<Unit>> {
        let rows: Vec<UnitRow> = sqlx::query_as(
            "SELECT id, name, unit_type, formation_kind, c2_capacity, personnel_mode, personnel_simplified_count FROM units ORDER BY id",
        )
        .fetch_all(self.pool)
        .await?;
        let mut units = Vec::with_capacity(rows.len());
        for row in rows {
            units.push(self.hydrate(row).await?);
        }
        Ok(units)
    }

    pub async fn get(&self, id: i64) -> Result<Option<Unit>> {
        let row: Option<UnitRow> = sqlx::query_as(
            "SELECT id, name, unit_type, formation_kind, c2_capacity, personnel_mode, personnel_simplified_count FROM units WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(self.hydrate(row).await?))
    }

    async fn write_composition(&self, unit_id: i64, unit: &Unit) -> Result<()> {
        sqlx::query("DELETE FROM unit_assets WHERE unit_id = ?")
            .bind(unit_id)
            .execute(self.pool)
            .await?;
        for owned in &unit.own_assets {
            sqlx::query(
                "INSERT INTO unit_assets (unit_id, asset_id, quantity) VALUES (?, ?, ?)",
            )
            .bind(unit_id)
            .bind(owned.asset_id)
            .bind(owned.quantity as i64)
            .execute(self.pool)
            .await?;
        }

        sqlx::query("DELETE FROM unit_personnel WHERE unit_id = ?")
            .bind(unit_id)
            .execute(self.pool)
            .await?;
        if let PersonnelComposition::Detailed { entries } = &unit.personnel {
            for entry in entries {
                sqlx::query(
                    "INSERT INTO unit_personnel (unit_id, personnel_type_id, quantity) VALUES (?, ?, ?)",
                )
                .bind(unit_id)
                .bind(entry.personnel_type_id)
                .bind(entry.quantity as i64)
                .execute(self.pool)
                .await?;
            }
        }

        Ok(())
    }

    /// `unit.id` is ignored; the new row's id is returned.
    pub async fn create(&self, unit: &Unit) -> Result<i64> {
        let (personnel_mode, personnel_simplified_count) = match &unit.personnel {
            PersonnelComposition::Simplified { count } => ("simplified", *count as i64),
            PersonnelComposition::Detailed { .. } => ("detailed", 0),
        };

        let id = sqlx::query(
            "INSERT INTO units (name, unit_type, formation_kind, c2_capacity, personnel_mode, personnel_simplified_count) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&unit.name)
        .bind(unit_type_to_str(unit.unit_type))
        .bind(formation_kind_to_str(unit.formation_kind))
        .bind(unit.c2_capacity.map(|c| c as i64))
        .bind(personnel_mode)
        .bind(personnel_simplified_count)
        .execute(self.pool)
        .await?
        .last_insert_rowid();

        self.write_composition(id, unit).await?;

        Ok(id)
    }

    /// Returns `false` if no unit with this id exists.
    pub async fn update(&self, id: i64, unit: &Unit) -> Result<bool> {
        let (personnel_mode, personnel_simplified_count) = match &unit.personnel {
            PersonnelComposition::Simplified { count } => ("simplified", *count as i64),
            PersonnelComposition::Detailed { .. } => ("detailed", 0),
        };

        let result = sqlx::query(
            "UPDATE units SET name = ?, unit_type = ?, formation_kind = ?, c2_capacity = ?, personnel_mode = ?, personnel_simplified_count = ? WHERE id = ?",
        )
        .bind(&unit.name)
        .bind(unit_type_to_str(unit.unit_type))
        .bind(formation_kind_to_str(unit.formation_kind))
        .bind(unit.c2_capacity.map(|c| c as i64))
        .bind(personnel_mode)
        .bind(personnel_simplified_count)
        .bind(id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        self.write_composition(id, unit).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetRepo;
    use crate::personnel_types::PersonnelTypeRepo;

    async fn test_pool() -> SqlitePool {
        let pool = crate::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        pool
    }

    fn simplified_unit(name: &str) -> Unit {
        Unit {
            id: 0,
            name: name.to_string(),
            unit_type: UnitType::Line,
            formation_kind: FormationKind::Standing,
            own_assets: vec![],
            personnel: PersonnelComposition::Simplified { count: 9 },
            c2_capacity: None,
        }
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips_simplified_personnel_and_assets() {
        let pool = test_pool().await;
        let asset_repo = AssetRepo::new(&pool);
        let unit_repo = UnitRepo::new(&pool);

        let asset_id = asset_repo.create("Scout Car", "Light Wheeled", &[]).await.unwrap();

        let mut unit = simplified_unit("1st Recon Squad");
        unit.own_assets.push(UnitAsset {
            asset_id,
            quantity: 2,
        });
        unit.c2_capacity = Some(5);

        let id = unit_repo.create(&unit).await.unwrap();
        let fetched = unit_repo.get(id).await.unwrap().expect("unit exists");

        assert_eq!(fetched.name, "1st Recon Squad");
        assert_eq!(fetched.c2_capacity, Some(5));
        assert_eq!(fetched.own_assets.len(), 1);
        assert_eq!(fetched.own_assets[0].quantity, 2);
        assert!(matches!(
            fetched.personnel,
            PersonnelComposition::Simplified { count: 9 }
        ));
    }

    #[tokio::test]
    async fn create_and_fetch_round_trips_detailed_personnel() {
        let pool = test_pool().await;
        let personnel_repo = PersonnelTypeRepo::new(&pool);
        let unit_repo = UnitRepo::new(&pool);

        let rifleman_id = personnel_repo
            .create("Rifleman", None, 60.0, 10.0, 0.0, &[])
            .await
            .unwrap();

        let mut unit = simplified_unit("Rifle Squad");
        unit.personnel = PersonnelComposition::Detailed {
            entries: vec![UnitPersonnelEntry {
                personnel_type_id: rifleman_id,
                quantity: 8,
            }],
        };

        let id = unit_repo.create(&unit).await.unwrap();
        let fetched = unit_repo.get(id).await.unwrap().expect("unit exists");

        match fetched.personnel {
            PersonnelComposition::Detailed { entries } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].quantity, 8);
            }
            other => panic!("expected Detailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_replaces_composition() {
        let pool = test_pool().await;
        let asset_repo = AssetRepo::new(&pool);
        let unit_repo = UnitRepo::new(&pool);

        let asset_id = asset_repo.create("Scout Car", "Light Wheeled", &[]).await.unwrap();

        let unit = simplified_unit("Recon Team");
        let id = unit_repo.create(&unit).await.unwrap();

        let mut updated = simplified_unit("Recon Team (renamed)");
        updated.own_assets.push(UnitAsset {
            asset_id,
            quantity: 1,
        });
        updated.personnel = PersonnelComposition::Simplified { count: 3 };

        let ok = unit_repo.update(id, &updated).await.unwrap();
        assert!(ok);

        let fetched = unit_repo.get(id).await.unwrap().expect("unit exists");
        assert_eq!(fetched.name, "Recon Team (renamed)");
        assert_eq!(fetched.own_assets.len(), 1);
        assert!(matches!(
            fetched.personnel,
            PersonnelComposition::Simplified { count: 3 }
        ));
    }

    #[tokio::test]
    async fn update_missing_unit_returns_false() {
        let pool = test_pool().await;
        let unit_repo = UnitRepo::new(&pool);
        let unit = simplified_unit("Ghost");
        assert!(!unit_repo.update(999, &unit).await.unwrap());
    }

    #[tokio::test]
    async fn list_returns_all_units() {
        let pool = test_pool().await;
        let unit_repo = UnitRepo::new(&pool);
        unit_repo.create(&simplified_unit("A")).await.unwrap();
        unit_repo.create(&simplified_unit("B")).await.unwrap();

        let all = unit_repo.list().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "A");
        assert_eq!(all[1].name, "B");
    }
}
