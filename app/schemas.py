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
