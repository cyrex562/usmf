from typing import List, Dict
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from .models import Component, Unit, Asset
from .schemas import UnitResponse

# Hardcoded chassis limits for V1 (would be database driven in V3)
CHASSIS_SPECS = {
    "Heavy Tracked": {"max_weight": 8000, "max_space": 20, "base_cost": 5000},
    "Light Wheeled": {"max_weight": 2000, "max_space": 8, "base_cost": 1000},
    "Infantry Squad": {"max_weight": 500, "max_space": 10, "base_cost": 0},
}


async def validate_asset_logic(
    session: AsyncSession, chassis_type: str, component_ids: List[Dict[str, int]]
) -> Dict:
    """
    Calculates stats and verifies constraints.
    component_ids: list of {"id": 1, "quantity": 1}
    """

    # 1. Fetch Components
    ids = [c["id"] for c in component_ids]
    if not ids:
        stmt = select(Component).where(1 == 0)  # Empty result
    else:
        stmt = select(Component).where(Component.id.in_(ids))

    result = await session.execute(stmt)
    components_db = {c.id: c for c in result.scalars().all()}

    # 2. Aggregators
    total_weight = 0
    total_space = 0
    total_cost = CHASSIS_SPECS.get(chassis_type, {}).get("base_cost", 0)
    power_gen = 0
    power_draw = 0

    violations = []

    # 3. Calculate
    for item in component_ids:
        cid = item["id"]
        qty = item["quantity"]
        comp = components_db.get(cid)
        if not comp:
            violations.append(f"Component ID {cid} not found.")
            continue

        stats = comp.stats or {}

        total_weight += stats.get("weight", 0) * qty
        total_space += stats.get("space", 0) * qty
        total_cost += stats.get("cost", 0) * qty
        power_gen += stats.get("power_gen", 0) * qty
        power_draw += stats.get("power_draw", 0) * qty

    # 4. Check Constraints
    specs = CHASSIS_SPECS.get(chassis_type)
    if not specs:
        violations.append(f"Unknown Chassis Type: {chassis_type}")
    else:
        if total_weight > specs["max_weight"]:
            violations.append(f"Overweight: {total_weight}/{specs['max_weight']}")
        if total_space > specs["max_space"]:
            violations.append(f"No Space: {total_space}/{specs['max_space']}")

    if power_draw > power_gen:
        violations.append(
            f"Insufficient Power: Generating {power_gen}, Need {power_draw}"
        )

    return {
        "valid": len(violations) == 0,
        "violations": violations,
        "stats": {
            "weight": total_weight,
            "max_weight": specs["max_weight"] if specs else 0,
            "current_sapce": total_space,
            "max_space": specs["max_space"] if specs else 0,
            "power_gen": power_gen,
            "power_draw": power_draw,
            "cost": total_cost,
            "net_power": power_gen - power_draw,
        },
    }


async def calculate_unit_stats(session: AsyncSession, unit_id: int) -> Dict:
    """
    Recursively calculates stats for a unit tree.
    """
    # 1. Fetch Unit with children eagerly?
    # For now, let's do a recursive fetch or assume we have the tree loaded.
    # In async SQLAlchemy, lazy loading is tricky. Best to write a recursive CTE query or fetch all units and build tree in memory.
    # For V2 prototype, let's fetch all and build tree in memory (efficiency tradeoff).

    result = await session.execute(select(Unit))
    all_units = result.scalars().all()

    # Build map
    unit_map = {u.id: u for u in all_units}
    children_map = {}
    for u in all_units:
        if u.parent_id:
            children_map.setdefault(u.parent_id, []).append(u)

    # Also fetch all assets and components to calculate base stats
    # This is getting heavy, but for a prototype it's fine.
    # Optimization: In a real app, we'd cache stats on the Unit model.

    # For now, let's just implement the logic for a single node and its subtree assuming generic "supply" usage

    def aggregate(uid):
        unit = unit_map.get(uid)
        if not unit:
            return {"weight": 0, "cost": 0, "supply": 0, "personnel": 0}

        stats = {
            "weight": 0,
            "cost": 0,
            "supply": 0,
            "personnel": 100,
        }  # Placeholder personnel

        # If unit has an asset, add its weight/cost (Implementation TODO: fetch specific asset stats)
        # For this prototype, we'll estimate based on unit_type or asset_id presence
        if unit.asset_id:
            stats["weight"] += 5000  # Dummy value
            stats["cost"] += 1000  # Dummy value
            stats["supply"] += 50  # Daily supply consumption

        # Recurse
        children = children_map.get(uid, [])
        for child in children:
            child_stats = aggregate(child.id)
            stats["weight"] += child_stats["weight"]
            stats["cost"] += child_stats["cost"]
            stats["supply"] += child_stats["supply"]
            stats["personnel"] += child_stats["personnel"]

        return stats

    return aggregate(unit_id)


async def get_full_unit_tree(session: AsyncSession) -> List[UnitResponse]:
    result = await session.execute(select(Unit))
    all_units = result.scalars().all()

    nodes = {}
    # First pass: create nodes
    for u in all_units:
        nodes[u.id] = UnitResponse(
            id=u.id,
            name=u.name,
            unit_type=u.unit_type,
            parent_id=u.parent_id,
            asset_id=u.asset_id,
            children=[],
        )

    # Second pass: build tree
    roots = []
    for node in nodes.values():
        if node.parent_id and node.parent_id in nodes:
            nodes[node.parent_id].children.append(node)
        else:
            roots.append(node)

    return roots
