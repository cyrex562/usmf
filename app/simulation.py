from typing import Dict, List
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from .models import SimulationRun, UnitState, SimulationEvent, Scenario, Unit
from datetime import datetime
import random


class SimulationEngine:
    def __init__(self, session: AsyncSession, simulation_run_id: int):
        self.session = session
        self.simulation_run_id = simulation_run_id

    async def _load_simulation(self) -> SimulationRun:
        """Load simulation run from database"""
        result = await self.session.execute(
            select(SimulationRun).where(SimulationRun.id == self.simulation_run_id)
        )
        return result.scalars().first()

    async def run_turn(self):
        """Execute one turn of simulation"""
        # 1. Load current state
        sim_run = await self._load_simulation()
        if not sim_run or sim_run.status != "running":
            return

        current_turn = sim_run.current_turn + 1

        # 2. Process combat (simplified)
        await self._process_combat(sim_run, current_turn)

        # 3. Propagate morale
        await self._propagate_morale(sim_run, current_turn)

        # 4. Check supply
        await self._check_supply(sim_run, current_turn)

        # 5. Check C2 connectivity
        await self._check_c2(sim_run, current_turn)

        # 6. Update turn counter
        sim_run.current_turn = current_turn

        # 7. Check if simulation is complete
        if current_turn >= sim_run.max_turns:
            sim_run.status = "completed"
            sim_run.completed_at = datetime.utcnow()

        await self.session.commit()

    async def _process_combat(self, sim_run: SimulationRun, turn: int):
        """Simplified combat: random casualties based on force size"""
        # Get all unit states for previous turn
        result = await self.session.execute(
            select(UnitState).where(
                UnitState.simulation_run_id == sim_run.id,
                UnitState.turn_number == turn - 1,
            )
        )
        prev_states = {s.unit_id: s for s in result.scalars().all()}

        # Create new states for this turn with casualties
        for unit_id, prev_state in prev_states.items():
            if prev_state.is_destroyed:
                # Copy destroyed state
                new_state = UnitState(
                    simulation_run_id=sim_run.id,
                    unit_id=unit_id,
                    turn_number=turn,
                    personnel_count=0,
                    morale=0,
                    supply_level=prev_state.supply_level,
                    is_hq_connected=prev_state.is_hq_connected,
                    is_destroyed=True,
                    casualties_this_turn=0,
                )
            else:
                # Calculate casualties (1-5% per turn, simplified)
                casualties = int(
                    prev_state.personnel_count * random.uniform(0.01, 0.05)
                )
                new_personnel = max(0, prev_state.personnel_count - casualties)

                # Check if unit is destroyed (< 20% strength)
                original_strength = (
                    prev_state.personnel_count + prev_state.casualties_this_turn
                )
                is_destroyed = (
                    new_personnel < (original_strength * 0.2)
                    if original_strength > 0
                    else False
                )

                new_state = UnitState(
                    simulation_run_id=sim_run.id,
                    unit_id=unit_id,
                    turn_number=turn,
                    personnel_count=new_personnel,
                    morale=prev_state.morale,  # Will be updated in morale phase
                    supply_level=prev_state.supply_level,
                    is_hq_connected=prev_state.is_hq_connected,
                    is_destroyed=is_destroyed,
                    casualties_this_turn=casualties,
                )

                if casualties > 0:
                    event = SimulationEvent(
                        simulation_run_id=sim_run.id,
                        turn_number=turn,
                        event_type="combat",
                        unit_id=unit_id,
                        description=f"Unit took {casualties} casualties",
                        severity="info" if casualties < 10 else "warning",
                    )
                    self.session.add(event)

                if is_destroyed and not prev_state.is_destroyed:
                    event = SimulationEvent(
                        simulation_run_id=sim_run.id,
                        turn_number=turn,
                        event_type="unit_destroyed",
                        unit_id=unit_id,
                        description="Unit destroyed",
                        severity="critical",
                    )
                    self.session.add(event)

            self.session.add(new_state)

    async def _propagate_morale(self, sim_run: SimulationRun, turn: int):
        """Propagate morale effects up the unit tree"""
        # Get current turn states
        result = await self.session.execute(
            select(UnitState).where(
                UnitState.simulation_run_id == sim_run.id, UnitState.turn_number == turn
            )
        )
        states = {s.unit_id: s for s in result.scalars().all()}

        # Get unit tree structure
        result = await self.session.execute(select(Unit))
        units = {u.id: u for u in result.scalars().all()}

        # Build parent-child map
        children_map = {}
        for u in units.values():
            if u.parent_id:
                children_map.setdefault(u.parent_id, []).append(u.id)

        # Calculate morale impact from casualties
        for unit_id, state in states.items():
            if state.casualties_this_turn > 0 and not state.is_destroyed:
                # Morale loss: 1 point per 1% casualties
                total_personnel = state.personnel_count + state.casualties_this_turn
                if total_personnel > 0:
                    casualty_rate = state.casualties_this_turn / total_personnel
                    morale_loss = int(casualty_rate * 100)
                    state.morale = max(0, state.morale - morale_loss)

                    # Propagate to parent (50% of loss)
                    unit = units.get(unit_id)
                    if unit and unit.parent_id and unit.parent_id in states:
                        parent_state = states[unit.parent_id]
                        parent_morale_loss = morale_loss // 2
                        parent_state.morale = max(
                            0, parent_state.morale - parent_morale_loss
                        )

                        if parent_morale_loss > 10:
                            event = SimulationEvent(
                                simulation_run_id=sim_run.id,
                                turn_number=turn,
                                event_type="morale_propagation",
                                unit_id=unit.parent_id,
                                description=f"Morale shock from subordinate losses (-{parent_morale_loss})",
                                severity="warning",
                            )
                            self.session.add(event)

    async def _check_supply(self, sim_run: SimulationRun, turn: int):
        """Check supply consumption vs capacity"""
        result = await self.session.execute(
            select(UnitState).where(
                UnitState.simulation_run_id == sim_run.id, UnitState.turn_number == turn
            )
        )
        states = list(result.scalars().all())

        for state in states:
            if not state.is_destroyed:
                # Consume supply (10 per turn per 100 personnel)
                consumption = (state.personnel_count // 100) * 10
                state.supply_level = max(0, state.supply_level - consumption)

                # Check for supply exhaustion
                if state.supply_level == 0:
                    # Morale penalty for no supply
                    state.morale = max(0, state.morale - 20)

                    event = SimulationEvent(
                        simulation_run_id=sim_run.id,
                        turn_number=turn,
                        event_type="supply_exhausted",
                        unit_id=state.unit_id,
                        description="Unit out of supply (morale -20)",
                        severity="critical",
                    )
                    self.session.add(event)

    async def _check_c2(self, sim_run: SimulationRun, turn: int):
        """Check command & control connectivity"""
        # Get current states
        result = await self.session.execute(
            select(UnitState).where(
                UnitState.simulation_run_id == sim_run.id, UnitState.turn_number == turn
            )
        )
        states = {s.unit_id: s for s in result.scalars().all()}

        # Get unit tree
        result = await self.session.execute(select(Unit))
        units = {u.id: u for u in result.scalars().all()}

        # Check if HQ units are destroyed
        for unit_id, state in states.items():
            unit = units.get(unit_id)
            if unit and unit.parent_id:
                parent_state = states.get(unit.parent_id)
                if parent_state and parent_state.is_destroyed:
                    # Parent HQ destroyed - sever C2
                    if state.is_hq_connected:
                        state.is_hq_connected = False
                        state.morale = max(
                            0, state.morale - 30
                        )  # Severe morale penalty

                        event = SimulationEvent(
                            simulation_run_id=sim_run.id,
                            turn_number=turn,
                            event_type="c2_severed",
                            unit_id=unit_id,
                            description="C2 link severed - HQ destroyed (morale -30)",
                            severity="critical",
                        )
                        self.session.add(event)


async def get_unit_tree(session: AsyncSession, root_unit_id: int) -> List[Unit]:
    """Get all units in a tree starting from root"""
    result = await session.execute(select(Unit))
    all_units = result.scalars().all()

    # Build children map
    children_map = {}
    for u in all_units:
        if u.parent_id:
            children_map.setdefault(u.parent_id, []).append(u)

    # Recursively collect units
    units = []

    def collect(uid):
        unit = next((u for u in all_units if u.id == uid), None)
        if unit:
            units.append(unit)
            for child in children_map.get(uid, []):
                collect(child.id)

    collect(root_unit_id)
    return units
