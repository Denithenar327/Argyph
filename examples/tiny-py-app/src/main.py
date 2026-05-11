from .math import add, multiply
from .types import User, Status


class Greeter:
    """A friendly greeter class."""

    greeting: str

    def __init__(self, message: str) -> None:
        self.greeting = message

    def greet(self, user: User) -> str:
        return f"{self.greeting}, {user.name}!"


def create_greeter(message: str) -> Greeter:
    """Factory function for Greeter."""
    return Greeter(message)


DEFAULT_USER = User(name="World", age=42)


def main() -> None:
    g = create_greeter("Hello")
    user = DEFAULT_USER
    print(g.greet(user))
    print(f"Status: {Status.ACTIVE.value}")


if __name__ == "__main__":
    main()
