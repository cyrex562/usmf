# USMF

Unit and equipment design + map-based tactical simulation. See [`design_doc.md`](./design_doc.md)
for the full architecture; this is just the quickstart.

- `backend/` — Rust workspace (Axum + sqlx/SQLite): `usmf-core` (domain model), `usmf-sim`
  (hex pathfinding/LOS/turn engine), `usmf-db` (persistence), `usmf-api` (HTTP server).
- `frontend/` — Vue 3 + Vite + TypeScript SPA.
- `legacy/python_prototype/` — the original FastAPI/HTMX prototype this project is superseding.
- `old/` — pre-2020 design documents/spreadsheets, kept for domain reference.

## Backend

```
cd backend
cargo test --workspace   # unit tests for core/sim/db
cargo run -p usmf-api     # serves on :8080, creates ./usmf.db on first run
```

## Frontend

```
cd frontend
npm install
npm run dev               # serves on :5173, expects the API on :8080
```

Set `VITE_API_BASE_URL` if the backend isn't on the default `http://localhost:8080`.
