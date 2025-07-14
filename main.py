"""
Minimal smoke-test for the medic extension.

Run:
    python test_medic.py
Then open http://localhost:3000/health in a browser or curl.
"""

import random
import time
from typing import List, Protocol

from colonoscopy import (
    set_probe,
    StatusColor,
    ServiceStatus as Health,
)


class MedicRunnable(Protocol):
    async def health(self) -> Health: ...


class ClockChecker(MedicRunnable):
    """Always healthy; reports current time in the description."""

    async def health(self) -> Health:
        return Health(
            name="clock",
            status=StatusColor.Green,
            description=time.strftime("%H:%M:%S"),
        )


class FlakyChecker(MedicRunnable):
    """Randomly flips between GREEN / ORANGE / RED every call."""

    async def health(self) -> Health:
        status = random.choice([StatusColor.Green, StatusColor.Orange, StatusColor.Red])
        return Health(
            name="rng",
            status=status,
            description=f"Rolled {status}",
        )


def main() -> None:
    services: List[MedicRunnable] = [ClockChecker(), FlakyChecker()]
    set_probe(services)


if __name__ == "__main__":
    main()
