from sqlalchemy import Column, Integer, String, JSON, ForeignKey, DateTime, Boolean
from sqlalchemy.orm import relationship, backref
from .database import Base
from datetime import datetime


# Association Table
class AssetComponent(Base):
    __tablename__ = "asset_components"
    asset_id = Column(Integer, ForeignKey("assets.id"), primary_key=True)
    component_id = Column(Integer, ForeignKey("components.id"), primary_key=True)
    quantity = Column(Integer, default=1)

    # Relationships
    component = relationship("Component")


class Asset(Base):
    __tablename__ = "assets"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, index=True)
    chassis_type = Column(String)  # e.g., "Heavy Tracked", "Light Wheeled"

    # Relationships
    components = relationship("AssetComponent", backref="asset")
    units = relationship("Unit", back_populates="asset")


class Unit(Base):
    __tablename__ = "units"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, index=True)
    unit_type = Column(String)  # HQ, Line, Support, etc.
    parent_id = Column(Integer, ForeignKey("units.id"), nullable=True)
    asset_id = Column(Integer, ForeignKey("assets.id"), nullable=True)

    # Relationships
    children = relationship(
        "Unit",
        backref=backref("parent", remote_side=[id]),
        cascade="all, delete-orphan",
        lazy="selectin",
    )
    asset = relationship("Asset", back_populates="units")

    def __repr__(self):
        return f"<Unit(id={self.id}, name='{self.name}', type='{self.unit_type}')>"


class Component(Base):
    __tablename__ = "components"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, index=True)
    type = Column(String, index=True)  # e.g., "Weapon", "Engine", "Sensor"
    stats = Column(JSON)  # Stores weight, power_draw, cost, etc.

    def __repr__(self):
        return f"<Component(id={self.id}, name='{self.name}', type='{self.type}')>"


class Scenario(Base):
    __tablename__ = "scenarios"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, index=True)
    description = Column(String)
    terrain_type = Column(String)  # e.g., "Urban", "Desert", "Forest"
    weather = Column(String)  # e.g., "Clear", "Rain", "Snow"
    duration_hours = Column(Integer, default=24)
    created_at = Column(DateTime, default=datetime.utcnow)

    # Relationships
    forces = relationship(
        "ScenarioForce", back_populates="scenario", cascade="all, delete-orphan"
    )

    def __repr__(self):
        return f"<Scenario(id={self.id}, name='{self.name}')>"


class ScenarioForce(Base):
    __tablename__ = "scenario_forces"

    id = Column(Integer, primary_key=True, index=True)
    scenario_id = Column(Integer, ForeignKey("scenarios.id"))
    side_name = Column(String)  # e.g., "Blue Force", "Red Force"
    root_unit_id = Column(Integer, ForeignKey("units.id"))

    # Initial conditions
    starting_morale = Column(Integer, default=100)
    starting_supply = Column(Integer, default=1000)

    # Relationships
    scenario = relationship("Scenario", back_populates="forces")
    root_unit = relationship("Unit")

    def __repr__(self):
        return f"<ScenarioForce(id={self.id}, side='{self.side_name}')>"


class SimulationRun(Base):
    __tablename__ = "simulation_runs"

    id = Column(Integer, primary_key=True, index=True)
    scenario_id = Column(Integer, ForeignKey("scenarios.id"))
    status = Column(String)  # "running", "completed", "paused"
    current_turn = Column(Integer, default=0)
    max_turns = Column(Integer)  # Based on scenario duration
    started_at = Column(DateTime, default=datetime.utcnow)
    completed_at = Column(DateTime, nullable=True)

    # Relationships
    scenario = relationship("Scenario")
    unit_states = relationship(
        "UnitState", back_populates="simulation_run", cascade="all, delete-orphan"
    )
    events = relationship(
        "SimulationEvent", back_populates="simulation_run", cascade="all, delete-orphan"
    )

    def __repr__(self):
        return f"<SimulationRun(id={self.id}, scenario_id={self.scenario_id}, turn={self.current_turn})>"


class UnitState(Base):
    __tablename__ = "unit_states"

    id = Column(Integer, primary_key=True, index=True)
    simulation_run_id = Column(Integer, ForeignKey("simulation_runs.id"))
    unit_id = Column(Integer, ForeignKey("units.id"))
    turn_number = Column(Integer)

    # State variables
    personnel_count = Column(Integer)
    morale = Column(Integer)  # 0-100
    supply_level = Column(Integer)
    is_hq_connected = Column(Boolean, default=True)
    is_destroyed = Column(Boolean, default=False)
    casualties_this_turn = Column(Integer, default=0)

    # Relationships
    simulation_run = relationship("SimulationRun", back_populates="unit_states")
    unit = relationship("Unit")

    def __repr__(self):
        return f"<UnitState(unit_id={self.unit_id}, turn={self.turn_number}, morale={self.morale})>"


class SimulationEvent(Base):
    __tablename__ = "simulation_events"

    id = Column(Integer, primary_key=True, index=True)
    simulation_run_id = Column(Integer, ForeignKey("simulation_runs.id"))
    turn_number = Column(Integer)
    event_type = Column(
        String
    )  # "combat", "morale_break", "supply_exhausted", "c2_severed"
    unit_id = Column(Integer, ForeignKey("units.id"), nullable=True)
    description = Column(String)
    severity = Column(String)  # "info", "warning", "critical"

    # Relationships
    simulation_run = relationship("SimulationRun", back_populates="events")
    unit = relationship("Unit")

    def __repr__(self):
        return f"<SimulationEvent(turn={self.turn_number}, type='{self.event_type}')>"
