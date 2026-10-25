import { getNotesAction } from "@/modules/notes";
import { NoteList } from "@/features/notes";
import { NotebookPen } from "lucide-react";

export default async function HomePage() {
  const notes = await getNotesAction();
  return (
    <main>
      <h1>
        <NotebookPen /> Notes
      </h1>
      <NoteList initialNotes={notes} />
    </main>
  );
}
