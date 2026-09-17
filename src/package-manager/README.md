# `package-manager` Module

An automated dependency analyzer, manifest generator, and package installer for cloned views in `@com.ai.vn/cli`.

---

## 📁 File Structure

```
src/package-manager/
├── index.ts        # Facade function (`setupProjectEnvironment`)
├── scanner.ts      # AST & regex import scanner (`scanFilesForDependencies`)
├── generator.ts    # `package.json` & `tsconfig.json` manifest builder
├── installer.ts    # Package manager runner (`bun install` / `npm install`)
└── constants.ts    # Default framework dependencies, versions & core ignore lists
```

---

## ⚙️ How It Works

When a user clones a cloud view via `com clone <viewId>`:

1. **AST Dependency Scanning (`scanner.ts`)**:
   - Inspects all `.ts`, `.tsx`, `.js`, `.jsx` files in the project.
   - Discovers third-party packages (e.g. `lucide-react`, `framer-motion`, `axios`, `date-fns`).
   - Automatically filters out internal path aliases (`@/*`, `~/*`), relative imports (`./*`, `../*`), and built-in Node.js/Bun modules (`fs`, `path`, `http`, `bun:test`).

2. **Manifest Generation (`generator.ts`)**:
   - If `package.json` does not exist: Creates a clean manifest with standard scripts (`dev`, `build`, `start`), detected dependencies, and Tailwind CSS v4 setup.
   - If `tsconfig.json` does not exist: Generates a modern TypeScript configuration with `@/*` path mapping and `react-jsx` support.
   - If `app/globals.css` does not exist: Seeds `@import "tailwindcss";` with customizable brand colors.

3. **Package Installation (`installer.ts`)**:
   - Automatically runs `bun install` (or `npm install`) to ensure all dependencies are available locally for immediate execution.

---

## 🧪 Testing

Run all 18 package manager unit & integration tests:
```bash
bun test tests/package-manager.test.ts tests/scanner.test.ts tests/generator.test.ts
```
