# com-compiler.wasm

The com engine (`compiler-rs`) built for `wasm32-wasip1`, to run `com dev` without a native binary:
in the browser (bun-rust kernel), or in any WASI runtime.

```sh
bun run build:wasm         # → binaries/wasm/com-compiler.wasm + host.js, then a smoke test
bun run build:wasm --dev   # the smoke test also runs `com dev` in the wasm engine
```

## One source, two builds

The native engine (Windows, macOS, Linux) is built exactly as before (`build:rust`, `build:binaries`);
this crate is a separate workspace that native builds never compile. The engine reaches the OS only
through `compiler-rs/src/platform`:

| | Native | wasm |
|---|---|---|
| `platform::fs` | `tokio::fs` | `std::fs` over WASI (the host's VFS) |
| `platform::process` | `tokio::process` | the host's `process_*` imports (e.g. bun-rust process workers) |
| `platform::spawn_blocking` | tokio's blocking pool | a task on the single thread |
| `platform::notify` | `notify` (OS watcher) | changes reported by the host (`com_file_changed`) |
| `platform::redb` | `redb` on disk | in memory |
| HTTP | axum on a TCP port | `com_request` (the Router, without a socket) |
| HMR | `/_hmr` WebSocket | `com_hmr_take` |

Natively unavailable on wasm: `com build` / `com start` (they need sockets), WebP encoding in
`next/image` (libwebp is C: images keep their format), response compression.

## Host

`host.ts` (`ComEngine`) loads the reactor and gives it files (a WASI implementation), child processes
(`spawn`), requests (`fetch(Request)`) and HMR (`onHmr`). Responses are buffered whole (no streaming
yet). See `smoke.ts` for a complete host on `node:wasi`.

Exports (`src/lib.rs`): `com_es5`, `com_tailwind`, `com_routes` (the CLI's JSON), `com_dev_start`,
`com_request`, `com_tick`, `com_job_take`, `com_hmr_take`, `com_file_changed`, `com_wake`,
`com_alloc` / `com_free`. Imports (module `com_host`): `process_spawn`, `process_write`,
`process_close_stdin`, `process_read`, `process_wait`, `process_kill`.
