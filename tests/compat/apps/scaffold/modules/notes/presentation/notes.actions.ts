"use server";

import { db } from "core";

export async function getNotesAction() {
  return await db.selectFrom("notes").selectAll().orderBy("id").execute();
}

export async function addNoteAction(title: string) {
  await db.insertInto("notes").values({ title }).execute();
  return await getNotesAction();
}
