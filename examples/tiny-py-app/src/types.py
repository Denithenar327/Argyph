"""Type definitions."""

from dataclasses import dataclass
from enum import Enum


@dataclass
class User:
    name: str
    age: int
    email: str | None = None


class Status(Enum):
    ACTIVE = "ACTIVE"
    INACTIVE = "INACTIVE"
    PENDING = "PENDING"


def format_user(user: User) -> str:
    return f"{user.name} ({user.age})"
