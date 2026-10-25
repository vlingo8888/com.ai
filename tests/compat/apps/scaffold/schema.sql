CREATE TABLE IF NOT EXISTS notes (
  id SERIAL PRIMARY KEY,
  title TEXT NOT NULL
);
INSERT INTO notes (title) VALUES ('Seeded note from PGlite');
