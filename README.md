# @com.ai.vn/cli (`com`)

> The Intelligent Web & Cloud Engine Developer Tooling for Next.js App Router & Server Actions.

`@com.ai.vn/cli` is a high-performance developer suite and local preview engine built with **Bun** and **Pure Rust**. It allows developers to clone, preview, and build Next.js App Router views with native Server Actions / Backend Modules with sub-millisecond hot reloading and near-zero memory footprint.

---

## 🚀 Key Highlights

- **🦀 Pure Rust Dev Engine (`com-compiler`)**: 50x-100x faster than standard `next dev`. Cold starts in `< 20ms` and consumes less than `25MB` RAM.
- **📁 Senior-Grade Modular App Router**: File-system routing supporting exact static routes, dynamic segments `[id]`, catch-all `[...slug]`, optional catch-all `[[...slug]]`, route groups `(group)`, and parallel slots `@slot`.
- **⚡ Smart TsConfig Path Resolver**: Dynamically parses `tsconfig.json` / `jsconfig.json` path mappings (`@/*`, `~/*`, `@components/*`, `@ui/*`) with JSONC comment stripping and path traversal protection.
- **🔄 Server Actions via Async RPC**: Automatically transforms server module imports (`modules/...`) into typed async RPC proxy stubs that seamlessly dispatch to backend execution sandbox.
- **🎨 Tailwind CSS v4 Engine**: Native browser compilation with JIT styling and custom design tokens.
- **📦 Intelligent Package Manager**: AST scanner that inspects project files, discovers third-party imports (`lucide-react`, `framer-motion`, `axios`, etc.), and generates optimized `package.json` and `tsconfig.json` manifests.
- **🔌 Automatic Port Conflict Resolution**: Intelligently switches to the next available port (`3000` -> `3001` -> `3002`) if port 3000 is occupied.

---

## 📦 Architecture Overview

```
packages/cli/
├── bin/
│   └── cli.ts                   # CLI entrypoint (com / cai / aivn)
├── compiler-rs/                 # 🦀 Standalone Pure Rust Compiler & Preview Engine
│   ├── src/
│   │   ├── bundler.rs           # Client HTML shell generator with Babel TSX transpiler
│   │   ├── resolver/            # Smart File & Import Resolver (TsConfig paths, extensions)
│   │   ├── router/              # App Router Scanner, Matcher & Precedence Engine (235+ tests)
│   │   ├── rpc.rs               # Server Action Dispatcher & Module Execution Sandbox
│   │   ├── server.rs            # Axum HTTP Server & Tokio WebSocket HMR Engine
│   │   └── main.rs              # Standalone `com-compiler` CLI runner
│   └── Cargo.toml
├── src/
│   ├── commands/                # CLI commands (login, whoami, logout, clone, dev)
│   ├── core/                    # Config, Auth Token storage, Logger & UI Cards
│   └── package-manager/         # Dependency AST scanner, manifest generator & installer
└── tests/                       # TypeScript unit & integration test suites
```

---

## 🛠️ Installation & Setup

### Global Installation via Bun
```bash
cd packages/cli
bun run build
bun link
```

### CLI Command Aliases
You can invoke the CLI using any of the following aliases:
- `com` (Primary)
- `cai` (Short alias)
- `aivn` (Alternative alias)

---

## 💻 CLI Commands

### 1. `com login`
Authenticates against Central Auth (`https://auth.myworkbeast.com/auth/google`) using an interactive local OAuth callback server.

```bash
com login
```

### 2. `com whoami`
Displays current authenticated user details and active session tokens.

```bash
com whoami
```

### 3. `com clone <viewId> [targetDir]`
Clones a cloud-hosted App Router view, runs the automated AST package scanner, writes missing `package.json` / `tsconfig.json` manifests, and installs dependencies.

```bash
com clone 6745abcdef123456 ./my-app
```

### 4. `com dev [--dir <path>] [--port <port>]`
Starts the pure Rust App Router compiler & dev preview engine.

```bash
cd my-app
com dev
```

---

## 🧪 Testing

### Rust Compiler & Router Tests (288 tests)
```bash
cd compiler-rs
cargo test
```

### TypeScript Package Manager Tests (18 tests)
```bash
bun test
```

---

## 📄 License
MIT © Com.AI.VN / NATA Team
