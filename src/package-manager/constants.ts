/**
 * Constants & presets for package management and manifest generation
 */

export const BUILT_IN_MODULES = new Set([
  "fs",
  "fs/promises",
  "path",
  "path/posix",
  "path/win32",
  "os",
  "http",
  "https",
  "http2",
  "crypto",
  "stream",
  "stream/promises",
  "events",
  "url",
  "util",
  "util/types",
  "buffer",
  "zlib",
  "child_process",
  "net",
  "tls",
  "dgram",
  "dns",
  "dns/promises",
  "readline",
  "perf_hooks",
  "worker_threads",
  "cluster",
  "v8",
  "vm",
  "wasi",
  "bun",
  "bun:test",
  "bun:sqlite",
  "bun:ffi",
  "bun:jsc",
  "bun:wrap",
]);

export const DEFAULT_DEPENDENCIES: Record<string, string> = {
  "react": "^18.3.1",
  "react-dom": "^18.3.1",
  "lucide-react": "^0.460.0",
  "clsx": "^2.1.1",
  "tailwind-merge": "^2.5.4",
  "kysely": "^0.27.4",
  "pg": "^8.13.1",
};

export const DEFAULT_DEV_DEPENDENCIES: Record<string, string> = {
  "tailwindcss": "^4.0.0",
  "@tailwindcss/postcss": "^4.0.0",
  "@types/react": "^18.3.12",
  "@types/react-dom": "^18.3.1",
  "@types/pg": "^8.11.10",
  "@types/bun": "latest",
  "typescript": "^5.6.3",
};

export const TAILWIND_V4_CSS_TEMPLATE = `@import "tailwindcss";

@theme {
  --color-brand-50: #f0f9ff;
  --color-brand-500: #0ea5e9;
  --color-brand-600: #0284c7;
  --color-brand-700: #0369a1;
}
`;
