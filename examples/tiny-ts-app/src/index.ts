import { add, multiply } from "./math";
import type { User } from "./types";

export interface Person {
  name: string;
  age: number;
}

export class Greeter {
  private greeting: string;

  constructor(message: string) {
    this.greeting = message;
  }

  greet(user: Person): string {
    return `${this.greeting}, ${user.name}!`;
  }
}

export function createGreeter(message: string): Greeter {
  return new Greeter(message);
}

const defaultUser: Person = { name: "World", age: 42 };

console.log(createGreeter("Hello").greet(defaultUser));
console.log(`Math: ${add(2, 3)} * 2 = ${multiply(add(2, 3), 2)}`);
