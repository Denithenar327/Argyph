export interface User {
  name: string;
  age: number;
  email?: string;
}

export type Role = "admin" | "user" | "guest";

export enum Status {
  Active = "ACTIVE",
  Inactive = "INACTIVE",
  Pending = "PENDING",
}

export function formatUser(user: User): string {
  return `${user.name} (${user.age})`;
}
