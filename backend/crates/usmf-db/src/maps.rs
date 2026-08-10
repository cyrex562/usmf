use anyhow::{bail, Result};
use sqlx::{FromRow, SqlitePool};
use usmf_core::{HexCell, HexCoord, Map, TerrainType};

fn terrain_to_str(t: TerrainType) -> &'static str {
    match t {
        TerrainType::Plains => "plains",
        TerrainType::Forest => "forest",
        TerrainType::Urban => "urban",
        TerrainType::Water => "water",
        TerrainType::Hill => "hill",
        TerrainType::Road => "road",
    }
}

fn terrain_from_str(s: &str) -> Result<TerrainType> {
    Ok(match s {
        "plains" => TerrainType::Plains,
        "forest" => TerrainType::Forest,
        "urban" => TerrainType::Urban,
        "water" => TerrainType::Water,
        "hill" => TerrainType::Hill,
        "road" => TerrainType::Road,
        other => bail!("unknown terrain '{other}' in database"),
    })
}

#[derive(FromRow)]
struct MapRow {
    id: i64,
    name: String,
    width: i64,
    height: i64,
}

#[derive(FromRow)]
struct HexCellRow {
    q: i64,
    r: i64,
    terrain: String,
    elevation: i64,
}

pub struct MapRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MapRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    async fn load_cells(&self, map_id: i64) -> Result<Vec<HexCell>> {
        let rows: Vec<HexCellRow> = sqlx::query_as(
            "SELECT q, r, terrain, elevation FROM hex_cells WHERE map_id = ? ORDER BY q, r",
        )
        .bind(map_id)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(HexCell {
                    coord: HexCoord::new(row.q as i32, row.r as i32),
                    terrain: terrain_from_str(&row.terrain)?,
                    elevation: row.elevation as i32,
                })
            })
            .collect()
    }

    async fn hydrate(&self, row: MapRow) -> Result<Map> {
        let cells = self.load_cells(row.id).await?;
        Ok(Map {
            id: row.id,
            name: row.name,
            width: row.width as u32,
            height: row.height as u32,
            cells,
        })
    }

    /// Listed without cells -- a Map Editor's picker view doesn't need every
    /// hex just to show a name/size, and a "few hundred hexes" map (per
    /// design_doc.md §4.2) times N maps in one list response isn't worth
    /// paying for until something actually needs it.
    pub async fn list(&self) -> Result<Vec<Map>> {
        let rows: Vec<MapRow> =
            sqlx::query_as("SELECT id, name, width, height FROM maps ORDER BY id")
                .fetch_all(self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|row| Map {
                id: row.id,
                name: row.name,
                width: row.width as u32,
                height: row.height as u32,
                cells: Vec::new(),
            })
            .collect())
    }

    pub async fn get(&self, id: i64) -> Result<Option<Map>> {
        let row: Option<MapRow> =
            sqlx::query_as("SELECT id, name, width, height FROM maps WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(self.hydrate(row).await?))
    }

    async fn write_cells(&self, map_id: i64, cells: &[HexCell]) -> Result<()> {
        sqlx::query("DELETE FROM hex_cells WHERE map_id = ?")
            .bind(map_id)
            .execute(self.pool)
            .await?;
        for cell in cells {
            sqlx::query(
                "INSERT INTO hex_cells (map_id, q, r, terrain, elevation) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(map_id)
            .bind(cell.coord.q)
            .bind(cell.coord.r)
            .bind(terrain_to_str(cell.terrain))
            .bind(cell.elevation)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    /// `map.id` is ignored; the new row's id is returned.
    pub async fn create(&self, map: &Map) -> Result<i64> {
        let id = sqlx::query("INSERT INTO maps (name, width, height) VALUES (?, ?, ?)")
            .bind(&map.name)
            .bind(map.width as i64)
            .bind(map.height as i64)
            .execute(self.pool)
            .await?
            .last_insert_rowid();

        self.write_cells(id, &map.cells).await?;

        Ok(id)
    }

    /// Returns `false` if no map with this id exists. Replaces every cell --
    /// same delete-then-insert-all shape as `UnitRepo::write_composition`,
    /// since a terrain-painting edit touches an unpredictable subset of
    /// hexes and there's no per-cell identity worth diffing against for a
    /// map this size.
    pub async fn update(&self, id: i64, map: &Map) -> Result<bool> {
        let result = sqlx::query("UPDATE maps SET name = ?, width = ?, height = ? WHERE id = ?")
            .bind(&map.name)
            .bind(map.width as i64)
            .bind(map.height as i64)
            .bind(id)
            .execute(self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        self.write_cells(id, &map.cells).await?;
        Ok(true)
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

    fn sample_map() -> Map {
        Map {
            id: 0,
            name: "Test Valley".to_string(),
            width: 3,
            height: 2,
            cells: vec![
                HexCell {
                    coord: HexCoord::new(0, 0),
                    terrain: TerrainType::Plains,
                    elevation: 0,
                },
                HexCell {
                    coord: HexCoord::new(1, 0),
                    terrain: TerrainType::Forest,
                    elevation: 1,
                },
                HexCell {
                    coord: HexCoord::new(0, 1),
                    terrain: TerrainType::Water,
                    elevation: 0,
                },
            ],
        }
    }

    #[tokio::test]
    async fn create_and_get_round_trips_cells() {
        let pool = test_pool().await;
        let repo = MapRepo::new(&pool);
        let id = repo.create(&sample_map()).await.unwrap();

        let fetched = repo.get(id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "Test Valley");
        assert_eq!(fetched.width, 3);
        assert_eq!(fetched.height, 2);
        assert_eq!(fetched.cells.len(), 3);
        let forest = fetched
            .cell_at(&HexCoord::new(1, 0))
            .expect("forest cell present");
        assert_eq!(forest.terrain, TerrainType::Forest);
        assert_eq!(forest.elevation, 1);
    }

    #[tokio::test]
    async fn list_omits_cells() {
        let pool = test_pool().await;
        let repo = MapRepo::new(&pool);
        repo.create(&sample_map()).await.unwrap();

        let maps = repo.list().await.unwrap();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].name, "Test Valley");
        assert!(maps[0].cells.is_empty());
    }

    #[tokio::test]
    async fn update_replaces_cells_entirely() {
        let pool = test_pool().await;
        let repo = MapRepo::new(&pool);
        let id = repo.create(&sample_map()).await.unwrap();

        let mut edited = sample_map();
        edited.id = id;
        edited.name = "Test Valley (revised)".to_string();
        edited.cells = vec![HexCell {
            coord: HexCoord::new(5, 5),
            terrain: TerrainType::Road,
            elevation: 0,
        }];

        let updated = repo.update(id, &edited).await.unwrap();
        assert!(updated);

        let fetched = repo.get(id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "Test Valley (revised)");
        assert_eq!(fetched.cells.len(), 1);
        assert_eq!(fetched.cells[0].coord, HexCoord::new(5, 5));
    }

    #[tokio::test]
    async fn update_returns_false_for_missing_map() {
        let pool = test_pool().await;
        let repo = MapRepo::new(&pool);
        let updated = repo.update(999, &sample_map()).await.unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn get_returns_none_for_missing_map() {
        let pool = test_pool().await;
        let repo = MapRepo::new(&pool);
        assert!(repo.get(999).await.unwrap().is_none());
    }
}
