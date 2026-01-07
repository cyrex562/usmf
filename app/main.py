from fastapi import FastAPI, Request, Depends
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from fastapi.responses import HTMLResponse
import os
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from .database import engine, Base, get_db
from .models import Component, Asset, Unit
from .services import validate_asset_logic, CHASSIS_SPECS, calculate_unit_stats
from .schemas import AssetCreate, AssetValidationResponse, UnitCreate, UnitResponse
from fastapi import HTTPException

app = FastAPI(title="USMF Proving Ground V2")

# Mount static files
if not os.path.exists("app/static"):
    os.makedirs("app/static")
app.mount("/static", StaticFiles(directory="app/static"), name="static")

templates = Jinja2Templates(directory="app/templates")


@app.on_event("startup")
async def startup():
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)


@app.get("/", response_class=HTMLResponse)
async def read_root(request: Request):
    return templates.TemplateResponse("base.html", {"request": request})


@app.get("/asset-designer", response_class=HTMLResponse)
async def asset_designer(request: Request, db: AsyncSession = Depends(get_db)):
    # Fetch components for the library list
    result = await db.execute(select(Component))
    components = result.scalars().all()

    return templates.TemplateResponse(
        "asset_designer.html",
        {
            "request": request,
            "components": components,
            "chassis_types": CHASSIS_SPECS.keys(),
        },
    )


@app.post("/api/assets/validate", response_class=HTMLResponse)
async def validate_asset(request: Request, db: AsyncSession = Depends(get_db)):
    form_data = await request.form()

    # Parse form data manually since it is HTMX
    # We expect fields like chassis_type, component_id_1, quantity_1, etc.
    chassis_type = form_data.get("chassis_type")

    component_ids = []
    for key, value in form_data.items():
        if key.startswith("component_id_"):
            idx = key.split("_")[-1]
            qty_key = f"quantity_{idx}"
            if qty_key in form_data:
                try:
                    component_ids.append(
                        {"id": int(value), "quantity": int(form_data.get(qty_key, 1))}
                    )
                except ValueError:
                    continue

    validation_result = await validate_asset_logic(db, chassis_type, component_ids)

    return templates.TemplateResponse(
        "partials/asset_stats.html",
        {
            "request": request,
            "stats": validation_result["stats"],
            "valid": validation_result["valid"],
            "violations": validation_result["violations"],
        },
    )


@app.get("/unit-designer", response_class=HTMLResponse)
async def unit_designer(request: Request, db: AsyncSession = Depends(get_db)):
    from .services import get_full_unit_tree  # lazy import to avoid circular if any

    roots = await get_full_unit_tree(db)

    # Fetch unassigned assets (assets not linked to any unit)
    # For V1 simplified: just list all assets
    asset_result = await db.execute(select(Asset))
    assets = asset_result.scalars().all()

    return templates.TemplateResponse(
        "unit_designer.html", {"request": request, "roots": roots, "assets": assets}
    )


@app.post("/api/units", response_model=UnitResponse)
async def create_unit(unit: UnitCreate, db: AsyncSession = Depends(get_db)):
    db_unit = Unit(**unit.dict())
    db.add(db_unit)
    await db.commit()
    await db.refresh(db_unit)
    return db_unit


@app.patch("/api/units/{unit_id}/move")
async def move_unit(unit_id: int, request: Request, db: AsyncSession = Depends(get_db)):
    data = await request.json()
    new_parent_id = data.get("parent_id")

    result = await db.execute(select(Unit).where(Unit.id == unit_id))
    unit = result.scalars().first()
    if not unit:
        raise HTTPException(status_code=404, detail="Unit not found")

    unit.parent_id = new_parent_id
    await db.commit()
    return {"status": "ok"}


@app.get("/api/units/{unit_id}/stats", response_class=HTMLResponse)
async def get_unit_stats(
    request: Request, unit_id: int, db: AsyncSession = Depends(get_db)
):
    stats = await calculate_unit_stats(db, unit_id)
    return templates.TemplateResponse(
        "partials/unit_stats.html", {"request": request, "stats": stats}
    )
