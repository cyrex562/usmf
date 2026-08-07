from pydantic import BaseModel
from typing import List, Dict, Optional


class AssetComponentCreate(BaseModel):
    component_id: int
    quantity: int = 1


class AssetCreate(BaseModel):
    name: str
    chassis_type: str
    components: List[AssetComponentCreate] = []


class AssetValidationResponse(BaseModel):
    valid: bool
    violations: List[str]
    stats: Dict[str, float]


class UnitCreate(BaseModel):
    name: str
    unit_type: str = "Line"
    parent_id: Optional[int] = None
    asset_id: Optional[int] = None


class UnitUpdate(BaseModel):
    parent_id: Optional[int] = None
    name: Optional[str] = None


class UnitResponse(BaseModel):
    id: int
    name: str
    unit_type: str
    parent_id: Optional[int]
    asset_id: Optional[int]
    children: List["UnitResponse"] = []

    class Config:
        orm_mode = True


class ScenarioForceCreate(BaseModel):
    side_name: str
    root_unit_id: int
    starting_morale: int = 100
    starting_supply: int = 1000


class ScenarioForceResponse(BaseModel):
    id: int
    side_name: str
    root_unit_id: int
    starting_morale: int
    starting_supply: int

    class Config:
        from_attributes = True


class ScenarioCreate(BaseModel):
    name: str
    description: str
    terrain_type: str
    weather: str = "Clear"
    duration_hours: int = 24
    forces: List[ScenarioForceCreate]


class ScenarioResponse(BaseModel):
    id: int
    name: str
    description: str
    terrain_type: str
    weather: str
    duration_hours: int
    forces: List[ScenarioForceResponse]

    class Config:
        from_attributes = True
