from sqlalchemy import Column, Integer, String, JSON, ForeignKey
from sqlalchemy.orm import relationship, backref
from .database import Base


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
