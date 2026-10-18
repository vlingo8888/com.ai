// Next.js-style hints for client-only React code that runs as a Server Component.
//
// Server modules (`rsc:` specifiers) resolve "react" with the `react-server` condition, which does not
// export createContext, useState, useEffect, ... A file without "use client" that a Server Component
// imports then fails with a bare `React.createContext is not a function`. Next.js names the API and the
// fix instead; this rewrites the error the same way, adding the file that runs on the server.

/** React APIs missing from the `react-server` build (React 19), as in `swc_compiler/module.rs` */
const CLIENT_ONLY_APIS = [
  "Component",
  "PureComponent",
  "createContext",
  "useContext",
  "useState",
  "useReducer",
  "useRef",
  "useEffect",
  "useLayoutEffect",
  "useInsertionEffect",
  "useImperativeHandle",
  "useSyncExternalStore",
  "useTransition",
  "useDeferredValue",
  "useOptimistic",
  "useActionState",
  "useEffectEvent",
  "startTransition",
];

// `React.useState is not a function`, `useState is not a function`, `(0, import_react.useState) is not a function`
const NOT_A_FUNCTION = new RegExp(`(?:^|[^\\w$])(${CLIENT_ONLY_APIS.join("|")})\\)? is not a function`);
// A named import that only fails to link (the compiler rejects it in the app's own files)
const MISSING_EXPORT = new RegExp(`Export named '(${CLIENT_ONLY_APIS.join("|")})' not found in module '[^']*/react/[^']*react-server[^']*'`);
// `at rsc:/abs/file.tsx?v=2:20:30` (or `(rsc:/abs/file.tsx:20:30)`)
const RSC_FRAME = /rsc:(\/[^\s?:)]+)(?:\?[^\s:)]*)?:(\d+)/;

const EXPLAINED = Symbol.for("nata.explainedError");

function hintFor(api: string): string {
  if (api === "createContext") {
    return (
      'createContext only works in Client Components. Add the "use client" directive at the top of the file to use it.\n' +
      "Read more: https://nextjs.org/docs/messages/context-in-server-component"
    );
  }
  const kind = api.startsWith("use") ? "React Hook" : "React API";
  return (
    `You're importing a component that needs \`${api}\`. This ${kind} only works in a Client Component. ` +
    'To fix, mark the file (or its parent) with the `"use client"` directive.\n' +
    "Learn more: https://nextjs.org/docs/app/api-reference/directives/use-client"
  );
}

function relativeToCwd(file: string): string {
  const cwd = process.cwd().replace(/\/$/, "") + "/";
  return file.startsWith(cwd) ? file.slice(cwd.length) : file;
}

/**
 * Rewrites (in place) an error thrown because a client-only React API was called in the Server
 * Components layer. Other errors are returned untouched.
 */
export function explainServerError<T>(err: T): T {
  if (!(err instanceof Error) || (err as any)[EXPLAINED]) return err;
  const original = err.message;
  const called = NOT_A_FUNCTION.exec(original)?.[1];
  const frame = called ? RSC_FRAME.exec(err.stack ?? "") : null;
  const api = frame ? called : MISSING_EXPORT.exec(original)?.[1];
  if (!api) return err;
  const where = frame
    ? `${relativeToCwd(frame[1])}:${frame[2]} runs as a Server Component: it has no "use client" directive and is imported`
    : `A file that imports \`${api}\` from "react" runs as a Server Component: it has no "use client" directive and is imported`;
  const message =
    `${hintFor(api)}\n\n` +
    `  ${where} from a Server Component (a page, a layout or another file without "use client").\n\n` +
    `  Original error: ${original}`;

  Object.defineProperty(err, EXPLAINED, { value: true });
  err.message = message;
  if (typeof err.stack === "string") {
    const head = `${err.name}: ${original}`;
    err.stack = err.stack.startsWith(head)
      ? `${err.name}: ${message}${err.stack.slice(head.length)}`
      : `${err.name}: ${message}\n${err.stack}`;
  }
  return err;
}
