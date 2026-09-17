# `compiler-rs` — High-Performance Pure Rust App Router & Dev Engine

A native Rust compiler and dev server designed for instantaneous preview of Next.js App Router applications, Server Actions, and Tailwind CSS v4.

---

## ⚡ Why Rust?

Traditional JavaScript dev servers (Node.js + Webpack/Turbopack) suffer from slow startup times (1.5s - 5.0s+), heavy memory footprints (300MB - 1GB+), and Garbage Collection stutter. `compiler-rs` leverages Axum, Tokio, and native OS APIs to deliver:
- **Instant Cold Start**: `< 15ms`
- **Low Memory Footprint**: `< 25MB` RAM
- **Microsecond Routing**: Compiled regex and radix precedence matching in `5-20µs`
- **Zero Configuration**: Automatic port negotiation and TsConfig path resolution

---

## 🏛️ Architecture & Modules

```
compiler-rs/src/
├── router/       # App Router Scanner, Matcher, and Segment Parser (235+ tests)
├── resolver/     # File on Disk & TsConfig Path Alias Resolver (53+ tests)
├── bundler.rs    # HTML Shell & Client-Side Babel TSX Transpiler Integrator
├── rpc.rs        # Server Actions / Module RPC Dispatcher
├── server.rs     # Axum HTTP Server & Tokio Broadcast WebSocket HMR
└── main.rs       # Standalone CLI entrypoint (`com-compiler dev` / `routes`)
```

---

## 🧩 Core Submodules

### 1. `router` Module (`src/router/`)
Scans the `app/` (or `src/app/`) directory and constructs a deterministic routing table according to Next.js App Router conventions:
- **Static Pages**: `/about`, `/contact`, `/pricing`
- **Dynamic Segments**: `/users/[id]`, `/shop/[category]/[productId]`
- **Catch-All**: `/docs/[...slug]`
- **Optional Catch-All**: `/blog/[[...slug]]`
- **Route Groups**: `app/(marketing)/about/page.tsx` -> `/about`
- **Parallel Slots**: `app/@modal/login/page.tsx`
- **Cascade Layouts**: Automatically aggregates `layout.tsx` files from root to leaf page.

### 2. `resolver` Module (`src/resolver/`)
Handles all file on disk resolution and TypeScript path mapping:
- **TsConfig Parser**: Reads `tsconfig.json` / `jsconfig.json` (with full JSONC comment support) and maps aliases like `@/*`, `~/*`, `@components/*`, `@ui/*`.
- **Extension Matching**: Automatically resolves `.tsx`, `.ts`, `.jsx`, `.js`, and `/index.*` files.
- **Import Transformer**: Rewrites relative (`./`, `../`) and path alias (`@/`) imports to absolute `/_bundle/...` endpoints.
- **Server Action Stubs**: Rewrites `import { action } from "modules/..."` into async RPC proxy callers.

### 3. `server` Module (`src/server.rs`)
Powered by `axum` and `tokio`:
- **Auto Port Fallback**: Automatically catches `AddrInUse` (OS error 48) and increments port (`3000` -> `3001` -> `3002`).
- **Endpoint `/_bundle/*`**: Serves transformed TypeScript/TSX files.
- **Endpoint `/_nata/rpc`**: Handles POST RPC calls for Server Actions.
- **Endpoint `/_hmr`**: High-speed WebSocket for live file reload notifications.

---

## 🚀 Usage

### Run Dev Server
```bash
cargo run --release -- dev --dir /path/to/project --port 3000
```

### Inspect Discovered Routes
```bash
cargo run --release -- routes --dir /path/to/project
```

### Run Test Suite (288 tests)
```bash
cargo test
```
