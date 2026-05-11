export function add(a: number, b: number): number {
  return a + b;
}

export function multiply(a: number, b: number): number {
  return a * b;
}

export type Operation = "add" | "multiply";

export function compute(op: Operation, a: number, b: number): number {
  switch (op) {
    case "add":
      return add(a, b);
    case "multiply":
      return multiply(a, b);
  }
}
