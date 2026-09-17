# Pure Rust Server-Side Rendering Engine (`src/ssr/`)

A high-performance Server-Side Rendering (SSR) module designed for instant App Router and cascading Layout evaluation.

---

## 📁 File Structure

```
src/ssr/
├── mod.rs          # Public facade (SsrEngine, SsrRequest, SsrOutput, SsrMode)
├── types.rs        # Core data structures and options
├── engine.rs       # SSR Engine orchestrator and CSR fallback pipeline
├── worker.rs       # Server-side execution runner
├── runtime.ts      # React App Router evaluator & renderToString
├── hydration.rs    # Client hydration script generator
├── tests.rs        # Unit & integration tests
└── README.md       # Architecture & Performance documentation
```

---

## ⚡ Key Features

1. **Sub-millisecond Routing & Context**: Route params, query strings, and headers are parsed natively in Rust before dispatching to SSR.
2. **Cascade Layout Tree Rendering**: Automatically aggregates root, route-group, and leaf layouts from outside-in.
3. **Graceful Fallback**: If an unhandled client-only browser API is executed on the server, the engine seamlessly falls back to Client-Side Rendering (CSR) without crashing the request.
4. **Instant Hydration**: Emits pre-rendered HTML directly inside `<div id="root">` with matching state serialized in `__NATA_SSR_DATA__`.
