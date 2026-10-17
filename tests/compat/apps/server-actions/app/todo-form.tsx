"use client";
import { useActionState, useState } from "react";
import { addTodo, greet } from "./actions";

export function TodoForm() {
  const [state, formAction, pending] = useActionState(addTodo, { todos: ["Read the docs"] });
  const [greeting, setGreeting] = useState("");
  return (
    <div>
      <form action={formAction}>
        <input name="title" defaultValue="Ship it" />
        <button type="submit" disabled={pending}>Add</button>
      </form>
      <ul id="todos">{state.todos.map((t) => <li key={t}>{t}</li>)}</ul>
      <button id="greet" onClick={async () => setGreeting(await greet("Zalo"))}>Greet</button>
      <p id="greeting">{greeting}</p>
    </div>
  );
}
