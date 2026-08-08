# USMF

Unit and equipment design + map-based tactical simulation. See [`design_doc.md`](./design_doc.md)
for the full architecture; this is just the quickstart.

- `backend/` — Rust workspace (Axum + sqlx/SQLite): `usmf-core` (domain model), `usmf-sim`
  (hex pathfinding/LOS/turn engine), `usmf-db` (persistence), `usmf-api` (HTTP server).
- `frontend/` — Vue 3 + Vite + TypeScript SPA.
- `legacy/python_prototype/` — the original FastAPI/HTMX prototype this project is superseding.
- `old/` — pre-2020 design documents/spreadsheets, kept for domain reference.

## Quickstart (`cargo xtask`)

`backend/crates/xtask` wraps the cargo/npm commands below into single commands, runnable from the
repo root (alias in `.cargo/config.toml`):

```
cargo xtask dev       # backend (:8080) + frontend (:5173) dev servers together, hot reload
cargo xtask run       # build + run the single embedded release binary (serves everything on :8080)
cargo xtask build     # build frontend dist/ and the backend workspace
cargo xtask test      # cargo test --workspace
cargo xtask lint      # clippy (both feature sets) + cargo fmt --check
cargo xtask ci        # test + lint + build -- everything that should be clean before committing
```

Run `cargo xtask` with no arguments for the full list. The sections below are the equivalent
manual commands, useful if you want to run just one tool directly.

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

## Release build (single binary)

For a deployable build, the Vue SPA is embedded into the `usmf-api` binary and served from the
same `:8080` server -- no separate frontend server needed. Build the frontend first (`usmf-api`
embeds whatever is in `frontend/dist` at compile time), then build the backend with the
`serve-frontend` feature:

```
cd frontend && npm install && npm run build
cd ../backend && cargo build --release -p usmf-api --features serve-frontend
```

Run the resulting `target/release/usmf-api` binary; it serves the API under `/api/*` and `/health`,
and falls back to the embedded SPA (with `index.html` for client-side routes) for everything else.

This feature is off by default, so plain `cargo build`/`cargo run` for local development is
unaffected and doesn't require `frontend/dist` to exist.
