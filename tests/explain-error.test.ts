import { describe, expect, it } from "bun:test";
import { explainServerError } from "../compiler-rs/src/worker_pool/explain_error";

/** The error JavaScriptCore throws for `call()`, with a stack frame in `file` */
function thrown(call: () => unknown, file: string): Error {
  try {
    call();
  } catch (err: any) {
    err.stack = `${err.name}: ${err.message}\n    at ${file}:20:30\n    at processTicksAndRejections (native:7:39)`;
    return err;
  }
  throw new Error("expected a throw");
}

const React: any = {};
const serverFile = `rsc:${process.cwd()}/components/ui/sidebar.tsx?v=3`;

describe("explainServerError", () => {
  it("explains createContext in a Server Component like Next.js, naming the file", () => {
    const err = explainServerError(thrown(() => React.createContext(null), serverFile));

    expect(err.message).toStartWith('createContext only works in Client Components. Add the "use client" directive');
    expect(err.message).toContain("https://nextjs.org/docs/messages/context-in-server-component");
    expect(err.message).toContain("components/ui/sidebar.tsx:20 runs as a Server Component");
    expect(err.message).toContain("Original error: React.createContext is not a function");
    expect(err.stack).toStartWith("TypeError: createContext only works in Client Components.");
    expect(err.stack).toContain(`at ${serverFile}:20:30`);
  });

  it("explains hooks, also through named imports and transpiled calls", () => {
    const { useState } = React;
    const named = explainServerError(thrown(() => useState(0), serverFile));
    expect(named.message).toStartWith("You're importing a component that needs `useState`. This React Hook only works in a Client Component.");

    const transpiled = new TypeError("(0, import_react.useEffect) is not a function");
    transpiled.stack = `TypeError: ${transpiled.message}\n    at ${serverFile}:5:1`;
    expect(explainServerError(transpiled).message).toContain("needs `useEffect`");
  });

  it("leaves unrelated errors and client-layer errors alone, and explains only once", () => {
    const other = thrown(() => React.somethingElse(), serverFile);
    expect(explainServerError(other).message).toBe(other.message);

    const ssr = thrown(() => React.useState(0), `ssr:${process.cwd()}/components/a.tsx`);
    const before = ssr.message;
    expect(explainServerError(ssr).message).toBe(before);

    const twice = explainServerError(explainServerError(thrown(() => React.useRef(null), serverFile)));
    expect(twice.message.match(/Original error/g)).toHaveLength(1);
    expect(explainServerError("not an error")).toBe("not an error");
  });

  it("explains a client-only named import that fails to link inside a package", () => {
    const err = new SyntaxError(
      `Export named 'useState' not found in module 'rsc:${process.cwd()}/node_modules/react/react.react-server.js'.`
    );
    explainServerError(err);
    expect(err.message).toStartWith("You're importing a component that needs `useState`.");
    expect(err.message).toContain('A file that imports `useState` from "react" runs as a Server Component');
    expect(Object.keys(err)).toEqual([]);
  });
});
