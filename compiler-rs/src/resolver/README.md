# `resolver` Module

The `resolver` module is responsible for file discovery on disk, TypeScript configuration (`tsconfig.json` / `jsconfig.json`) path alias resolution, and browser ESM import rewriting.

---

## 📁 File Structure

```
src/resolver/
├── mod.rs        # Public facade (PathResolver, FileResolver, ImportResolver, TsConfig)
├── types.rs      # ResolvedFile and ImportKind enums
├── fs.rs         # File on disk locator, candidate extension generator & traversal protection
├── imports.rs    # Import specifier classifier and AST source transformer
├── tsconfig.rs   # JSONC parser for tsconfig.json / jsconfig.json compilerOptions.paths
└── tests.rs      # Unit & integration tests for all resolver features (53+ tests)
```

---

## ⚙️ How It Works

### 1. File on Disk Resolution (`FileResolver`)
When a file is requested through `GET /_bundle/*` (e.g. `/_bundle/components/Header`):
1. **Candidate Generation**: The resolver checks the following candidate paths in priority order:
   - `components/Header`
   - `components/Header.tsx`
   - `components/Header.ts`
   - `components/Header.jsx`
   - `components/Header.js`
   - `components/Header/index.tsx`
   - `components/Header/index.ts`
   - `components/Header/index.jsx`
   - `components/Header/index.js`
   - `src/components/Header.tsx` (Fallback if project uses `src/` directory)
2. **Directory Traversal Protection**: Rejects any path containing `..` that attempts to escape outside the workspace root.

### 2. `tsconfig.json` & `jsconfig.json` Parsing (`TsConfig`)
- Parses `compilerOptions.baseUrl` and `compilerOptions.paths`.
- Strips single-line (`//`) and multi-line (`/* */`) comments automatically (JSONC).
- Supports wildcard path aliases:
  ```json
  {
    "compilerOptions": {
      "baseUrl": ".",
      "paths": {
        "@/*": ["./src/*", "./*"],
        "@components/*": ["./components/*"],
        "@ui/*": ["./components/ui/*"],
        "@utils": ["./lib/utils.ts"]
      }
    }
  }
  ```

### 3. Import Specifier Rewriting (`ImportResolver`)
Transforms all import statements in `.tsx` and `.ts` files into browser-executable ESM formats:

| Original Import | Transformed Browser ESM Import |
| :--- | :--- |
| `import Header from "./Header"` *(in `app/page.tsx`)* | `import Header from "/_bundle/app/Header"` |
| `import { Button } from "@/components/ui/button"` | `import { Button } from "/_bundle/components/ui/button"` |
| `import { motion } from "framer-motion"` | `import { motion } from "https://esm.sh/framer-motion@11.11.17"` |
| `import { Heart } from "lucide-react"` | `import { Heart } from "https://esm.sh/lucide-react@0.460.0"` |
| `import "./globals.css"` | `/* css import */` *(Inert comment)* |
| `import { getArticles } from "modules/news/use-cases"` | Generates Typed Async RPC Client Proxy |

---

## 🧪 Testing

Run all 53+ resolver unit & integration tests:
```bash
cargo test resolver::tests
```
