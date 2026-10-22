/**
 * Bun evaluates a module graph that has a cycle and top-level await (the database client) wrongly when
 * several import() calls enter it at once: all but the first see its bindings uninitialized ("Cannot
 * access 'pub' before initialization"). A fresh worker, or a new module version after an edit, gets
 * parallel requests (a page calls several actions), so first imports of server modules run one at a time.
 * The worker installs `serialImport()` as `globalThis.__nata_import` for the runtimes.
 */
const imported = new Map<string, Promise<unknown>>();
let importing: Promise<unknown> = Promise.resolve();

/** How long a first import waits for the one before it: a module whose evaluation waits on another
 * request of this worker would otherwise wait forever */
export const SERIAL_IMPORT_WAIT_MS = 5000;

export function serialImport(specifier: string): Promise<any> {
  const done = imported.get(specifier);
  if (done) return done;
  const turn = Promise.race([importing, new Promise((resolve) => setTimeout(resolve, SERIAL_IMPORT_WAIT_MS))]);
  const loading = turn.then(() => import(specifier));
  imported.set(specifier, loading);
  importing = loading.catch(() => {});
  // A failed import is attempted again by the next request
  loading.catch(() => imported.delete(specifier));
  return loading;
}
