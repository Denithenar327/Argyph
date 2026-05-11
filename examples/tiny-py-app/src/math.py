"""Math utility functions."""


def add(a: int, b: int) -> int:
    return a + b


def multiply(a: int, b: int) -> int:
    return a * b


def compute(op: str, a: int, b: int) -> int:
    if op == "add":
        return add(a, b)
    elif op == "multiply":
        return multiply(a, b)
    raise ValueError(f"Unknown operation: {op}")
