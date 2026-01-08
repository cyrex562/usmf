from fastapi import FastAPI, Request, Depends
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from fastapi.responses import HTMLResponse
import os
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from .database import engine, Base, get_db
from .models import (
    Component,
    Asset,
    Unit,
    Scenario,
    ScenarioForce,
    SimulationRun,
    UnitState,
    SimulationEvent,
)
from .services import validate_asset_logic, CHASSIS_SPECS, calculate_unit_stats
from .schemas import (
    AssetCreate,
    AssetValidationResponse,
    UnitCreate,
    UnitResponse,
    ScenarioCreate,
    ScenarioResponse,
)
from fastapi import HTTPException
from .simulation import SimulationEngine, get_unit_tree

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


@app.get("/scenarios", response_class=HTMLResponse)
async def list_scenarios(request: Request, db: AsyncSession = Depends(get_db)):
    """Display all scenarios"""
    result = await db.execute(select(Scenario))
    scenarios = result.scalars().all()
    return templates.TemplateResponse(
        "scenarios.html", {"request": request, "scenarios": scenarios}
    )


@app.get("/scenario-designer", response_class=HTMLResponse)
async def scenario_designer(request: Request, db: AsyncSession = Depends(get_db)):
    """Scenario creation interface"""
    # Fetch all root units (units without parents) for selection
    result = await db.execute(select(Unit).where(Unit.parent_id == None))
    root_units = result.scalars().all()

    return templates.TemplateResponse(
        "scenario_designer.html",
        {
            "request": request,
            "root_units": root_units,
            "terrain_types": ["Urban", "Desert", "Forest", "Mountain", "Coastal"],
            "weather_options": ["Clear", "Rain", "Snow", "Fog", "Storm"],
        },
    )


@app.post("/api/scenarios", response_model=ScenarioResponse)
async def create_scenario(scenario: ScenarioCreate, db: AsyncSession = Depends(get_db)):
    """Create a new scenario"""
    db_scenario = Scenario(
        name=scenario.name,
        description=scenario.description,
        terrain_type=scenario.terrain_type,
        weather=scenario.weather,
        duration_hours=scenario.duration_hours,
    )
    db.add(db_scenario)
    await db.flush()  # Get the scenario ID

    # Add forces
    for force_data in scenario.forces:
        db_force = ScenarioForce(scenario_id=db_scenario.id, **force_data.dict())
        db.add(db_force)

    await db.commit()
    await db.refresh(db_scenario)
    return db_scenario


@app.get("/api/scenarios/{scenario_id}", response_model=ScenarioResponse)
async def get_scenario(scenario_id: int, db: AsyncSession = Depends(get_db)):
    """Get scenario details"""
    result = await db.execute(select(Scenario).where(Scenario.id == scenario_id))
    scenario = result.scalars().first()
    if not scenario:
        raise HTTPException(status_code=404, detail="Scenario not found")
    return scenario


@app.delete("/api/scenarios/{scenario_id}")
async def delete_scenario(scenario_id: int, db: AsyncSession = Depends(get_db)):
    """Delete a scenario"""
    result = await db.execute(select(Scenario).where(Scenario.id == scenario_id))
    scenario = result.scalars().first()
    if not scenario:
        raise HTTPException(status_code=404, detail="Scenario not found")

    await db.delete(scenario)
    await db.commit()
    return {"status": "deleted"}


@app.post("/api/scenarios/{scenario_id}/simulate")
async def start_simulation(scenario_id: int, db: AsyncSession = Depends(get_db)):
    """Start a new simulation run"""
    # Get scenario
    result = await db.execute(select(Scenario).where(Scenario.id == scenario_id))
    scenario = result.scalars().first()
    if not scenario:
        raise HTTPException(status_code=404, detail="Scenario not found")

    # Create simulation run
    max_turns = scenario.duration_hours  # 1 turn = 1 hour
    sim_run = SimulationRun(
        scenario_id=scenario_id, status="running", current_turn=0, max_turns=max_turns
    )
    db.add(sim_run)
    await db.flush()

    # Initialize unit states from scenario forces
    for force in scenario.forces:
        # Get all units in this force's tree
        units = await get_unit_tree(db, force.root_unit_id)

        for unit in units:
            unit_state = UnitState(
                simulation_run_id=sim_run.id,
                unit_id=unit.id,
                turn_number=0,
                personnel_count=100,  # Default starting strength
                morale=force.starting_morale,
                supply_level=force.starting_supply,
                is_hq_connected=True,
                is_destroyed=False,
            )
            db.add(unit_state)

    await db.commit()
    return {"simulation_id": sim_run.id, "status": "started"}


@app.post("/api/simulations/{sim_id}/step")
async def step_simulation(sim_id: int, db: AsyncSession = Depends(get_db)):
    """Execute one turn of simulation"""
    engine = SimulationEngine(db, sim_id)
    await engine.run_turn()

    # Get updated status
    result = await db.execute(select(SimulationRun).where(SimulationRun.id == sim_id))
    sim_run = result.scalars().first()

    return {
        "turn": sim_run.current_turn,
        "status": sim_run.status,
        "max_turns": sim_run.max_turns,
    }


@app.get("/api/simulations/{sim_id}/events")
async def get_simulation_events(sim_id: int, db: AsyncSession = Depends(get_db)):
    """Get all events for a simulation"""
    result = await db.execute(
        select(SimulationEvent)
        .where(SimulationEvent.simulation_run_id == sim_id)
        .order_by(SimulationEvent.turn_number, SimulationEvent.id)
    )
    events = result.scalars().all()
    return [
        {
            "turn_number": e.turn_number,
            "description": e.description,
            "severity": e.severity,
        }
        for e in events
    ]


@app.get("/simulation/{sim_id}", response_class=HTMLResponse)
async def view_simulation(
    sim_id: int, request: Request, db: AsyncSession = Depends(get_db)
):
    """View simulation interface"""
    result = await db.execute(select(SimulationRun).where(SimulationRun.id == sim_id))
    sim_run = result.scalars().first()
    if not sim_run:
        raise HTTPException(status_code=404, detail="Simulation not found")

    # Get scenario
    result = await db.execute(
        select(Scenario).where(Scenario.id == sim_run.scenario_id)
    )
    scenario = result.scalars().first()

    return templates.TemplateResponse(
        "simulation.html",
        {"request": request, "sim_run": sim_run, "scenario": scenario},
    )
