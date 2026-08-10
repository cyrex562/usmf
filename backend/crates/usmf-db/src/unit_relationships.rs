use std::collections::HashSet;

use anyhow::Result;
use sqlx::{FromRow, SqlitePool};
use usmf_core::{RelationshipRules, RelationshipTypeSpec, UnitRelationship};

#[derive(Debug, thiserror::Error)]
pub enum CreateRelationshipError {
    #[error("unknown relationship type '{0}'")]
    UnknownRelationshipType(String),
    #[error("unit {0} does not exist")]
    UnknownUnit(i64),
    #[error("this Organic relationship would create a cycle in the permanent command tree")]
    WouldCreateCycle,
    #[error("unit {0} already has an Organic superior; detach that relationship first")]
    AlreadyHasOrganicSuperior(i64),
    #[error(transparent)]
    Db(#[from] anyhow::Error),
}

#[derive(FromRow)]
struct RelationshipTypeSpecRow {
    name: String,
    includes_in_span_of_control: bool,
    sustainment_transfers: bool,
    includes_in_combat_power_rollup: bool,
}

impl From<RelationshipTypeSpecRow> for RelationshipTypeSpec {
    fn from(row: RelationshipTypeSpecRow) -> Self {
        RelationshipTypeSpec {
            name: row.name,
            rules: RelationshipRules {
                includes_in_span_of_control: row.includes_in_span_of_control,
                sustainment_transfers: row.sustainment_transfers,
                includes_in_combat_power_rollup: row.includes_in_combat_power_rollup,
            },
        }
    }
}

#[derive(FromRow)]
struct UnitRelationshipRow {
    id: i64,
    superior_unit_id: i64,
    subordinate_unit_id: i64,
    relationship_type: String,
    includes_in_span_of_control: bool,
    sustainment_transfers: bool,
    includes_in_combat_power_rollup: bool,
    effective_from_turn: Option<i64>,
    effective_until_turn: Option<i64>,
    notes: Option<String>,
}

impl From<UnitRelationshipRow> for UnitRelationship {
    fn from(row: UnitRelationshipRow) -> Self {
        UnitRelationship {
            id: row.id,
            superior_unit_id: row.superior_unit_id,
            subordinate_unit_id: row.subordinate_unit_id,
            relationship_type: row.relationship_type,
            rules: RelationshipRules {
                includes_in_span_of_control: row.includes_in_span_of_control,
                sustainment_transfers: row.sustainment_transfers,
                includes_in_combat_power_rollup: row.includes_in_combat_power_rollup,
            },
            effective_from_turn: row.effective_from_turn,
            effective_until_turn: row.effective_until_turn,
            notes: row.notes,
        }
    }
}

pub struct UnitRelationshipRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> UnitRelationshipRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_relationship_types(&self) -> Result<Vec<RelationshipTypeSpec>> {
        let rows: Vec<RelationshipTypeSpecRow> = sqlx::query_as(
            "SELECT name, includes_in_span_of_control, sustainment_transfers, includes_in_combat_power_rollup FROM relationship_type_specs ORDER BY name",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn create_relationship_type(&self, spec: &RelationshipTypeSpec) -> Result<()> {
        sqlx::query(
            "INSERT INTO relationship_type_specs (name, includes_in_span_of_control, sustainment_transfers, includes_in_combat_power_rollup) VALUES (?, ?, ?, ?)",
        )
        .bind(&spec.name)
        .bind(spec.rules.includes_in_span_of_control)
        .bind(spec.rules.sustainment_transfers)
        .bind(spec.rules.includes_in_combat_power_rollup)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<UnitRelationship>> {
        // Joins relationship_type_specs so every row carries its resolved
        // RelationshipRules alongside the raw type label -- matches
        // usmf_core::UnitRelationship's shape without a second query per row.
        let rows: Vec<UnitRelationshipRow> = sqlx::query_as(
            "SELECT
                r.id, r.superior_unit_id, r.subordinate_unit_id, r.relationship_type,
                s.includes_in_span_of_control, s.sustainment_transfers, s.includes_in_combat_power_rollup,
                r.effective_from_turn, r.effective_until_turn, r.notes
            FROM unit_relationships r
            JOIN relationship_type_specs s ON s.name = r.relationship_type
            ORDER BY r.id",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Every relationship where `unit_id` is either the superior or the
    /// subordinate -- "who do I command" and "who do I report to" in one call.
    pub async fn list_for_unit(&self, unit_id: i64) -> Result<Vec<UnitRelationship>> {
        let rows: Vec<UnitRelationshipRow> = sqlx::query_as(
            "SELECT
                r.id, r.superior_unit_id, r.subordinate_unit_id, r.relationship_type,
                s.includes_in_span_of_control, s.sustainment_transfers, s.includes_in_combat_power_rollup,
                r.effective_from_turn, r.effective_until_turn, r.notes
            FROM unit_relationships r
            JOIN relationship_type_specs s ON s.name = r.relationship_type
            WHERE r.superior_unit_id = ? OR r.subordinate_unit_id = ?
            ORDER BY r.id",
        )
        .bind(unit_id)
        .bind(unit_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Walks the *current* Organic chain upward from `start`, looking for
    /// `candidate`, against the given transaction (so it sees the same
    /// snapshot the rest of `create` validates and writes against). Used to
    /// reject an Organic link that would make `candidate` its own (in)direct
    /// superior. Bounded by a visited-set so a pre-existing data
    /// inconsistency can't spin this forever.
    async fn is_organic_ancestor_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        start: i64,
        candidate: i64,
    ) -> Result<bool> {
        let mut current = start;
        let mut visited = HashSet::new();
        loop {
            if current == candidate {
                return Ok(true);
            }
            if !visited.insert(current) {
                return Ok(false);
            }
            let parent: Option<(i64,)> = sqlx::query_as(
                "SELECT superior_unit_id FROM unit_relationships WHERE subordinate_unit_id = ? AND relationship_type = 'Organic'",
            )
            .bind(current)
            .fetch_optional(&mut **tx)
            .await?;
            match parent {
                Some((p,)) => current = p,
                None => return Ok(false),
            }
        }
    }

    /// Everything here runs inside one transaction so the existence/cycle/
    /// duplicate-parent checks and the insert see a consistent snapshot and
    /// commit atomically -- without it, two relationship creates racing each
    /// other could each pass validation against pre-insert state and
    /// together corrupt the permanent (supposed-to-be-acyclic,
    /// one-parent-per-unit) Organic tree. This covers this app's actual
    /// usage pattern (a single interactive client); it does not add
    /// `BEGIN IMMEDIATE`-level locking against a truly concurrent writer
    /// racing the same transaction start, which isn't a realistic scenario
    /// here.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        superior_unit_id: i64,
        subordinate_unit_id: i64,
        relationship_type: &str,
        effective_from_turn: Option<i64>,
        effective_until_turn: Option<i64>,
        notes: Option<&str>,
    ) -> Result<i64, CreateRelationshipError> {
        if superior_unit_id == subordinate_unit_id {
            return Err(CreateRelationshipError::WouldCreateCycle);
        }

        let mut tx = self.pool.begin().await.map_err(anyhow::Error::from)?;

        for unit_id in [superior_unit_id, subordinate_unit_id] {
            let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM units WHERE id = ?")
                .bind(unit_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
            if exists.is_none() {
                return Err(CreateRelationshipError::UnknownUnit(unit_id));
            }
        }

        let type_exists: Option<(String,)> =
            sqlx::query_as("SELECT name FROM relationship_type_specs WHERE name = ?")
                .bind(relationship_type)
                .fetch_optional(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
        if type_exists.is_none() {
            return Err(CreateRelationshipError::UnknownRelationshipType(
                relationship_type.to_string(),
            ));
        }

        if relationship_type == "Organic" {
            let existing_parent: Option<(i64,)> = sqlx::query_as(
                "SELECT superior_unit_id FROM unit_relationships WHERE subordinate_unit_id = ? AND relationship_type = 'Organic'",
            )
            .bind(subordinate_unit_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
            if existing_parent.is_some() {
                return Err(CreateRelationshipError::AlreadyHasOrganicSuperior(
                    subordinate_unit_id,
                ));
            }

            if Self::is_organic_ancestor_tx(&mut tx, superior_unit_id, subordinate_unit_id).await? {
                return Err(CreateRelationshipError::WouldCreateCycle);
            }
        }

        let id = sqlx::query(
            "INSERT INTO unit_relationships (superior_unit_id, subordinate_unit_id, relationship_type, effective_from_turn, effective_until_turn, notes) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(superior_unit_id)
        .bind(subordinate_unit_id)
        .bind(relationship_type)
        .bind(effective_from_turn)
        .bind(effective_until_turn)
        .bind(notes)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .last_insert_rowid();

        tx.commit().await.map_err(anyhow::Error::from)?;

        Ok(id)
    }

    /// Ends a relationship by setting its `effective_until_turn` -- the row
    /// (and its history) stays, unlike a delete. Returns `false` if no
    /// relationship with this id exists.
    pub async fn end(&self, id: i64, effective_until_turn: i64) -> Result<bool> {
        let result =
            sqlx::query("UPDATE unit_relationships SET effective_until_turn = ? WHERE id = ?")
                .bind(effective_until_turn)
                .bind(id)
                .execute(self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::UnitRepo;
    use usmf_core::{FormationKind, PersonnelComposition, Unit, UnitType};

    async fn test_pool() -> SqlitePool {
        let pool = crate::connect("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        pool
    }

    async fn make_unit(pool: &SqlitePool, name: &str) -> i64 {
        let repo = UnitRepo::new(pool);
        repo.create(&Unit {
            id: 0,
            name: name.to_string(),
            unit_type: UnitType::Line,
            formation_kind: FormationKind::Standing,
            own_assets: vec![],
            personnel: PersonnelComposition::Simplified { count: 0 },
            c2_capacity: None,
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn seed_data_has_the_six_doctrinal_types() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let types = repo.list_relationship_types().await.unwrap();
        assert_eq!(types.len(), 6);
        assert!(types
            .iter()
            .any(|t| t.name == "Organic" && t.rules == RelationshipRules::ORGANIC));
        assert!(types
            .iter()
            .any(|t| t.name == "OPCON" && t.rules == RelationshipRules::OPCON));
    }

    #[tokio::test]
    async fn create_relationship_type_adds_a_custom_type_usable_by_create() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let custom = RelationshipTypeSpec {
            name: "Liaison".to_string(),
            rules: RelationshipRules {
                includes_in_span_of_control: false,
                sustainment_transfers: false,
                includes_in_combat_power_rollup: false,
            },
        };
        repo.create_relationship_type(&custom).await.unwrap();

        let types = repo.list_relationship_types().await.unwrap();
        assert_eq!(types.len(), 7);
        assert!(types
            .iter()
            .any(|t| t.name == "Liaison" && t.rules == custom.rules));

        // The new type is immediately usable by create(), the same as any seeded one.
        let hq = make_unit(&pool, "Division HQ").await;
        let team = make_unit(&pool, "Liaison Team").await;
        repo.create(hq, team, "Liaison", None, None, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn create_and_list_round_trips_with_resolved_rules() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let bn = make_unit(&pool, "1st Battalion").await;
        let co = make_unit(&pool, "Alpha Company").await;

        repo.create(bn, co, "Organic", None, None, Some("standing TO&E"))
            .await
            .unwrap();

        let rels = repo.list_for_unit(bn).await.unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].superior_unit_id, bn);
        assert_eq!(rels[0].subordinate_unit_id, co);
        assert_eq!(rels[0].rules, RelationshipRules::ORGANIC);
        assert_eq!(rels[0].notes.as_deref(), Some("standing TO&E"));

        // Also visible from the subordinate's side.
        assert_eq!(repo.list_for_unit(co).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_relationship_type_is_rejected() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let a = make_unit(&pool, "A").await;
        let b = make_unit(&pool, "B").await;

        let err = repo
            .create(a, b, "Made Up Type", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            CreateRelationshipError::UnknownRelationshipType(_)
        ));
    }

    #[tokio::test]
    async fn self_relationship_is_rejected_as_a_cycle() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let a = make_unit(&pool, "A").await;

        let err = repo
            .create(a, a, "Organic", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRelationshipError::WouldCreateCycle));
    }

    #[tokio::test]
    async fn nonexistent_unit_is_rejected_not_a_500() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let a = make_unit(&pool, "A").await;

        let err = repo
            .create(a, 999, "Organic", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRelationshipError::UnknownUnit(999)));

        let err = repo
            .create(999, a, "Organic", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRelationshipError::UnknownUnit(999)));
    }

    #[tokio::test]
    async fn second_organic_superior_is_rejected() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let a = make_unit(&pool, "A").await;
        let b = make_unit(&pool, "B").await;
        let c = make_unit(&pool, "C").await;

        repo.create(a, c, "Organic", None, None, None)
            .await
            .unwrap();

        // C already organically reports to A; B trying to also organically
        // command C would give it two parents, which isn't a tree anymore.
        let err = repo
            .create(b, c, "Organic", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            CreateRelationshipError::AlreadyHasOrganicSuperior(id) if id == c
        ));

        // Non-organic types aren't subject to the one-parent constraint.
        repo.create(b, c, "OPCON", None, None, None).await.unwrap();
    }

    #[tokio::test]
    async fn organic_cycle_is_rejected() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let division = make_unit(&pool, "Division").await;
        let brigade = make_unit(&pool, "Brigade").await;
        let battalion = make_unit(&pool, "Battalion").await;

        // Division -> Brigade -> Battalion, organically.
        repo.create(division, brigade, "Organic", None, None, None)
            .await
            .unwrap();
        repo.create(brigade, battalion, "Organic", None, None, None)
            .await
            .unwrap();

        // Battalion organically "commanding" Division would close the loop.
        let err = repo
            .create(battalion, division, "Organic", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRelationshipError::WouldCreateCycle));
    }

    #[tokio::test]
    async fn non_organic_relationship_does_not_trigger_cycle_check() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let division = make_unit(&pool, "Division").await;
        let brigade = make_unit(&pool, "Brigade").await;

        repo.create(division, brigade, "Organic", None, None, None)
            .await
            .unwrap();

        // Brigade OPCONs a liaison element down from Division is fine even
        // though it's "against the grain" of the organic tree -- OPCON isn't
        // subject to the acyclic-permanent-tree constraint.
        repo.create(brigade, division, "OPCON", None, None, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn end_sets_effective_until_turn_without_deleting_the_row() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        let a = make_unit(&pool, "A").await;
        let b = make_unit(&pool, "B").await;
        let id = repo
            .create(a, b, "Attached", Some(5), None, None)
            .await
            .unwrap();

        assert!(repo.end(id, 10).await.unwrap());

        let rels = repo.list_for_unit(a).await.unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].effective_until_turn, Some(10));
        assert!(!rels[0].is_active_at(Some(12)));
        assert!(rels[0].is_active_at(Some(7)));
    }

    #[tokio::test]
    async fn end_missing_relationship_returns_false() {
        let pool = test_pool().await;
        let repo = UnitRelationshipRepo::new(&pool);
        assert!(!repo.end(999, 1).await.unwrap());
    }
}
