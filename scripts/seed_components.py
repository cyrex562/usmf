import asyncio
import sys
import os

# Add parent directory to path so we can import app modules
sys.path.append(os.getcwd())

from app.database import engine, SessionLocal, Base
from app.models import Component
from sqlalchemy import select

INITIAL_COMPONENTS = [
    {
        "name": "Standard Fusion Core",
        "type": "Power",
        "stats": {"weight": 500, "space": 4, "power_gen": 1000, "cost": 250},
    },
    {
        "name": "Compact Diesel Engine",
        "type": "Power",
        "stats": {"weight": 300, "space": 3, "power_gen": 400, "cost": 50},
    },
    {
        "name": "120mm Smoothbore Cannon",
        "type": "Weapon",
        "stats": {
            "weight": 1200,
            "space": 6,
            "power_draw": 10,
            "damage": 80,
            "cost": 1500,
        },
    },
    {
        "name": "7.62mm Machine Gun",
        "type": "Weapon",
        "stats": {"weight": 20, "space": 1, "power_draw": 0, "damage": 5, "cost": 100},
    },
    {
        "name": "Lidar Sensor array",
        "type": "Sensor",
        "stats": {
            "weight": 50,
            "space": 2,
            "power_draw": 50,
            "range": 2000,
            "cost": 800,
        },
    },
    {
        "name": "Optical Scope",
        "type": "Sensor",
        "stats": {"weight": 2, "space": 1, "power_draw": 0, "range": 800, "cost": 200},
    },
    {
        "name": "Standard Ration Pack",
        "type": "Logistics",
        "stats": {"weight": 1, "space": 0.1, "calories": 2000, "cost": 5},
    },
    {
        "name": "Fuel Jerrycan",
        "type": "Logistics",
        "stats": {"weight": 20, "space": 1, "fuel": 20, "cost": 15},
    },
    {
        "name": "Composite Armor Plate",
        "type": "Armor",
        "stats": {"weight": 100, "space": 1, "armor_value": 50, "cost": 300},
    },
    {
        "name": "Cyberdeck Mk1",
        "type": "Cyber",
        "stats": {
            "weight": 5,
            "space": 1,
            "power_draw": 20,
            "cyber_attack": 40,
            "cost": 1200,
        },
    },
]


async def seed():
    print("Seeding database...")
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    async with SessionLocal() as session:
        # Check if already seeded
        result = await session.execute(select(Component))
        existing = result.scalars().first()
        if existing:
            print("Database already contains data. Skipping.")
            return

        for data in INITIAL_COMPONENTS:
            comp = Component(**data)
            session.add(comp)
            print(f"Adding: {data['name']}")

        await session.commit()
    print("Seeding complete.")


if __name__ == "__main__":
    asyncio.run(seed())
