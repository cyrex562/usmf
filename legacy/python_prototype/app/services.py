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
    Recursively calculates stats for a unit tree, including capability aggregation.
    """
    # Fetch all units
    result = await session.execute(select(Unit))
    all_units = result.scalars().all()

    # Build unit maps
    unit_map = {u.id: u for u in all_units}
    children_map = {}
    for u in all_units:
        if u.parent_id:
            children_map.setdefault(u.parent_id, []).append(u)

    # Fetch all assets with their components for capability extraction
    from .models import AssetComponent

    result = await session.execute(select(Asset))
    all_assets = result.scalars().all()
    asset_map = {a.id: a for a in all_assets}

    # Build asset -> capabilities map
    asset_capabilities = {}
    for asset in all_assets:
        capabilities = {}
        # Fetch components for this asset
        result = await session.execute(
            select(AssetComponent).where(AssetComponent.asset_id == asset.id)
        )
        asset_components = result.scalars().all()

        for ac in asset_components:
            # Fetch the component details
            comp_result = await session.execute(
                select(Component).where(Component.id == ac.component_id)
            )
            component = comp_result.scalars().first()
            if component and component.stats:
                comp_caps = component.stats.get("capabilities", {})
                for cap_name, cap_level in comp_caps.items():
                    # Multiply by quantity and sum
                    capabilities[cap_name] = capabilities.get(cap_name, 0) + (
                        cap_level * ac.quantity
                    )

        asset_capabilities[asset.id] = capabilities

    def aggregate(uid):
        unit = unit_map.get(uid)
        if not unit:
            return {
                "weight": 0,
                "cost": 0,
                "supply": 0,
                "personnel": 0,
                "capabilities": {},
            }

        stats = {
            "weight": 0,
            "cost": 0,
            "supply": 0,
            "personnel": 100,
            "capabilities": {},
        }

        # If unit has an asset, add its stats and capabilities
        if unit.asset_id and unit.asset_id in asset_map:
            stats["weight"] += 5000  # Dummy value
            stats["cost"] += 1000  # Dummy value
            stats["supply"] += 50  # Daily supply consumption

            # Add asset capabilities
            asset_caps = asset_capabilities.get(unit.asset_id, {})
            for cap_name, cap_level in asset_caps.items():
                stats["capabilities"][cap_name] = (
                    stats["capabilities"].get(cap_name, 0) + cap_level
                )

        # Recurse through children
        children = children_map.get(uid, [])
        for child in children:
            child_stats = aggregate(child.id)
            stats["weight"] += child_stats["weight"]
            stats["cost"] += child_stats["cost"]
            stats["supply"] += child_stats["supply"]
            stats["personnel"] += child_stats["personnel"]

            # Aggregate capabilities from children
            for cap_name, cap_level in child_stats.get("capabilities", {}).items():
                stats["capabilities"][cap_name] = (
                    stats["capabilities"].get(cap_name, 0) + cap_level
                )

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
