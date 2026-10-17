"use server";

let todos: string[] = ["Read the docs"];

// Where the action actually ran: Server Actions must never execute in the browser
const runtime = () => (typeof window === "undefined" ? "server" : "browser");

export async function addTodo(_prev: { todos: string[] }, formData: FormData) {
  const title = String(formData.get("title") || "").trim();
  if (title) todos = [...todos, `${title} (added on the ${runtime()})`];
  return { todos };
}

export async function greet(name: string) {
  return `Hello, ${name} (ran on the ${runtime()})`;
}
