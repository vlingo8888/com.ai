"use client";

import { useState } from "react";
import { Plus } from "lucide-react";
import { addNoteAction } from "@/modules/notes";

export function NoteList({ initialNotes }: { initialNotes: { id: number; title: string }[] }) {
  const [notes, setNotes] = useState(initialNotes);
  return (
    <div>
      <ul>
        {notes.map((note) => (
          <li key={note.id}>{note.title}</li>
        ))}
      </ul>
      <button id="add" onClick={async () => setNotes(await addNoteAction("Added by a Server Action"))}>
        <Plus /> Add
      </button>
    </div>
  );
}
