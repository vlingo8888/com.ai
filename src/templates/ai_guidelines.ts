/**
 * Generates AGENTS.md documentation for AI coding assistants (Antigravity, Cursor, Copilot, Claude)
 * explaining the project architecture, `core` package, `modules` backend domain structure,
 * `features` frontend clean architecture, and best coding practices.
 */
export function generateAgentsGuide(viewName: string, viewId: number | string): string {
  return `# Clean Architecture & AI Coding Guidelines

> **Project:** ${viewName} (View #${viewId})  
> **Framework:** Com.AI.VN High-Performance App Router & Fullstack Engine (Next.js Compatible)

---

## 🚨 0. CRITICAL INVIOLABLE MANDATES FOR AI AGENTS (READ FIRST)

As an AI coding assistant working on this repository, you **MUST STRICTLY FOLLOW** these rules without exception:

1. **NO AD-HOC DATABASE CLIENTS**: NEVER install or instantiate \`pg\`, \`postgres\`, \`prisma\`, \`drizzle\`, or manual database connections. ALWAYS use \`import { db, sql, HttpError, truncateTables } from "core"\`.
2. **CLEAN ARCHITECTURE SEPARATION**:
   - Backend queries and domain business logic **MUST ALWAYS** live in \`modules/<domain>/\`.
   - Feature UI components, state dialogs, forms, and hooks **MUST ALWAYS** live in \`features/<feature>/\`.
   - Generic reusable UI primitives (Button, Input, Dialog, Table) **MUST ALWAYS** live in \`components/ui/\`.
   - App Router routes (\`app/[route]/page.tsx\`) **MUST REMAIN THIN**; only import and compose features and modules.
3. **ZERO-MOCK TESTING MANDATE**:
   - When running tests via \`com test\` (or \`bun test\`), \`core\` automatically activates an **in-memory PostgreSQL engine (@pglite/core)** and auto-loads \`schema.sql\`.
   - **DO NOT create manual mocks for database queries or Server Actions!** Import real Server Actions and use-cases directly in your test suites.
   - Colocate tests: place \`*.test.ts\` directly next to the use-case or action being tested.
4. **TYPE ACCURACY**: Reference \`types/db.d.ts\` and \`schema.sql\` for exact column names, nullability, and relations.

---

## 1. Directory Structure & Clean Architecture Overview

The codebase is strictly separated into **4 architectural layers**:

\`\`\`
├── app/                  # 1. Routing & Composition Layer (Next.js App Router)
│   ├── layout.tsx        # Root layout (Html, Body, Providers, CSS imports)
│   ├── page.tsx          # Home page
│   ├── globals.css       # Global styles (Tailwind CSS v4 & theme variables)
│   └── (dashboard)/...   # Route groups & page compositions
│
├── features/             # 2. Frontend Clean Architecture (Feature-Driven UI & State)
│   └── <feature_name>/   # E.g. news, payroll, products, auth
│       ├── ui/           # Feature UI components, Dialogs, Forms, Tables, Cards
│       ├── hooks/        # Custom React hooks (useNews, usePayrollFilter, etc.)
│       ├── model/        # UI view models, state types, form schemas
│       └── index.ts      # Public feature barrel export
│
├── modules/              # 3. Backend Clean Architecture (Domain Logic & Database)
│   └── <domain_name>/    # E.g. news, payroll, products, auth
│       ├── domain/       # Entity types, interfaces, Zod validation schemas
│       ├── use-cases/    # Pure business logic & Kysely database queries
│       ├── presentation/ # Server Actions & Controller endpoints
│       └── index.ts      # Exported Server Actions for frontend consumption
│
├── types/                # TypeScript global type definitions
│   └── db.d.ts           # Auto-generated Kysely Database Schema types (run \`com db pull\`)
├── schema.sql            # Auto-generated database DDL schema for AI context
├── components/           # 4. Shared UI Design System Primitives
│   └── ui/               # Generic reusable components (Button, Dialog, Input, Table, DatePicker)
├── lib/                  # Shared utilities (cn, date formatters, math helpers)
├── public/               # Static assets (images, icons, fonts)
├── .env                  # Environment configuration (DATABASE_URL, etc.)
└── AGENTS.md             # This AI architecture & coding guide
\`\`\`

---

## 2. The Canonical \`core\` Module (\`import ... from "core"\`)

The project provides a built-in, type-safe \`core\` backend module. Always import server utilities from \`"core"\`:

\`\`\`typescript
import { db, sql, HttpError, cookies, upload, auth } from "core";
\`\`\`

### A. Database Queries with Kysely (\`db\`)
\`db\` is a fully-typed Kysely query builder instance connected to PostgreSQL / PGlite:

\`\`\`typescript
import { db } from "core";

// 1. Select queries
export async function getNewsList() {
  return await db
    .selectFrom("news")
    .selectAll()
    .where("status", "=", "published")
    .orderBy("created_at", "desc")
    .execute();
}

// 2. Insert queries
export async function createNews(data: { title: string; content: string; author_id: number }) {
  return await db
    .insertInto("news")
    .values({
      title: data.title,
      content: data.content,
      author_id: data.author_id,
      created_at: new Date().toISOString(),
    })
    .returningAll()
    .executeTakeFirstOrThrow();
}

// 3. Update queries
export async function updateNews(id: number, data: Partial<{ title: string; content: string; status: string }>) {
  return await db
    .updateTable("news")
    .set(data)
    .where("id", "=", id)
    .returningAll()
    .executeTakeFirst();
}

// 4. Raw SQL with parameter binding
import { sql } from "core";
const result = await sql\`SELECT COUNT(*) as total FROM news WHERE status = 'published'\`.execute(db);
\`\`\`

### B. Standard HTTP Errors (\`HttpError\`)
Throw typed HTTP errors in actions and services for automatic status code handling:

\`\`\`typescript
import { HttpError } from "core";

if (!user) throw HttpError.notFound("User not found");
if (!isAdmin) throw HttpError.forbidden("Access denied");
if (isDuplicate) throw HttpError.conflict("Slug already exists");
if (invalidInput) throw HttpError.badRequest("Validation failed");
\`\`\`

### C. Server-side Cookies (\`cookies\`)
\`\`\`typescript
import { cookies } from "core";

const cookieStore = await cookies();
const token = cookieStore.get("session_token")?.value;
cookieStore.set("session_token", "jwt_value", { httpOnly: true, path: "/" });
cookieStore.delete("session_token");
\`\`\`

### D. File Uploads (\`upload\`)
\`\`\`typescript
import { upload } from "core";

const result = await upload(file, { folder: "news_thumbnails" });
// result = { url: "https://...", key: "news_thumbnails/...", size: 1024 }
\`\`\`

---

<!-- DATABASE_SCHEMA_START -->
## 3. Database Schema Overview (Live Synced)

> **Database Types:** \`types/db.d.ts\` | **Schema DDL:** \`schema.sql\`  
> AI coding assistants should reference \`types/db.d.ts\` for exact table interfaces and column types when writing database queries.  
> Run \`com db pull\` to refresh this section and update TypeScript definitions whenever database migrations occur.
<!-- DATABASE_SCHEMA_END -->

---

## 4. Backend Clean Architecture (\`modules/<domain>/\`)

All database operations and business logic MUST live inside \`modules/<domain>/\`.

### Example Structure: \`modules/news/\`
\`\`\`
modules/news/
├── domain/
│   └── news.types.ts           # Interfaces & Zod validation schemas
├── use-cases/
│   ├── get-news.use-case.ts    # Read operations & filters
│   └── create-news.use-case.ts # Write operations & business rules
└── index.ts                    # Public Server Actions exported to frontend
\`\`\`

### Example Code: \`modules/news/index.ts\`
\`\`\`typescript
"use server";

import { db, HttpError } from "core";

export interface CreateNewsInput {
  title: string;
  content: string;
  category?: string;
}

export async function getNewsListAction() {
  return await db.selectFrom("news").selectAll().orderBy("created_at", "desc").execute();
}

export async function createNewsAction(input: CreateNewsInput) {
  if (!input.title || input.title.trim().length === 0) {
    throw HttpError.badRequest("Title is required");
  }

  return await db
    .insertInto("news")
    .values({
      title: input.title,
      content: input.content || "",
      category: input.category || "General",
      created_at: new Date().toISOString(),
    })
    .returningAll()
    .executeTakeFirstOrThrow();
}
\`\`\`

---

## 4. Frontend Clean Architecture (\`features/<feature>/\`)

All feature-specific UI, dialogs, forms, tables, and custom hooks MUST live inside \`features/<feature>/\`.

### Example Structure: \`features/news/\`
\`\`\`
features/news/
├── ui/
│   ├── NewsTable.tsx           # Table display of news items
│   ├── NewsFormDialog.tsx      # Modal create/edit dialog with form inputs
│   └── NewsCard.tsx            # Card component for grid view
├── hooks/
│   └── useNews.ts              # Custom hook handling loading, optimistic state, and refresh
├── model/
│   └── news-form.schema.ts     # Client form validation types
└── index.ts                    # Feature barrel export
\`\`\`

### Example Code: \`features/news/ui/NewsFormDialog.tsx\`
\`\`\`tsx
"use client";

import { useState, useTransition } from "react";
import { createNewsAction } from "@/modules/news";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface NewsFormDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess?: () => void;
}

export function NewsFormDialog({ isOpen, onClose, onSuccess }: NewsFormDialogProps) {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [isPending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    startTransition(async () => {
      try {
        await createNewsAction({ title, content });
        setTitle("");
        setContent("");
        onSuccess?.();
        onClose();
      } catch (err: any) {
        setError(err.message || "Failed to create news");
      }
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <form onSubmit={handleSubmit} className="w-full max-w-md rounded-xl bg-card p-6 shadow-xl border">
        <h2 className="text-lg font-bold text-card-foreground mb-4">Create News Article</h2>
        {error && <div className="mb-3 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}
        <div className="space-y-4">
          <Input placeholder="Article Title" value={title} onChange={(e) => setTitle(e.target.value)} required />
          <textarea
            placeholder="Content"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            className="w-full rounded-lg border bg-background p-3 text-sm"
            rows={4}
          />
        </div>
        <div className="mt-6 flex justify-end gap-3">
          <Button type="button" variant="outline" onClick={onClose} disabled={isPending}>Cancel</Button>
          <Button type="submit" disabled={isPending}>{isPending ? "Saving..." : "Create News"}</Button>
        </div>
      </form>
    </div>
  );
}
\`\`\`

### Example Code: \`features/news/index.ts\`
\`\`\`typescript
export { NewsTable } from "./ui/NewsTable";
export { NewsFormDialog } from "./ui/NewsFormDialog";
export { NewsCard } from "./ui/NewsCard";
export { useNews } from "./hooks/useNews";
\`\`\`

---

## 5. Page Composition in App Router (\`app/\`)

Pages in \`app/\` should be **thin composition layers** that assemble features together:

### Example: \`app/dashboard/news/page.tsx\`
\`\`\`tsx
import { getNewsListAction } from "@/modules/news";
import { NewsTable, NewsFormDialog } from "@/features/news";

export default async function NewsManagementPage() {
  const initialNews = await getNewsListAction();

  return (
    <div className="container mx-auto p-6 space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">News Management</h1>
          <p className="text-muted-foreground text-sm">Create and manage published articles</p>
        </div>
      </div>

      {/* Feature UI table */}
      <NewsTable initialData={initialNews} />
    </div>
  );
}
\`\`\`

---

## 6. Zalo Mini App (ZMP) Guidelines

If the project includes \`zmp-sdk\` or is executed in Zalo Mini App mode (\`com dev --target zalo\`):
- Import ZMP APIs from \`"zmp-sdk"\` or \`"zmp-sdk/apis"\`:
  \`\`\`typescript
  import { getUserInfo, getPhoneNumber } from "zmp-sdk/apis";
  \`\`\`
- The dev engine automatically provides mock data for native ZMP APIs during local browser development.
- Package for deployment via \`com build --target zalo\` to generate \`dist/\` with \`app-config.json\`.

---

## 7. Unit, Integration & Component Testing Standards

The project uses the built-in **Bun Test Runner** via \`com test\` (alias \`com t\`). All test files MUST follow the conventions below:

### A. File Naming & Location Conventions (Colocated Tests)
- Place test files directly next to the files they test (e.g. \`calculate-salary.use-case.test.ts\` next to \`calculate-salary.use-case.ts\`).
- Test files must end with \`*.test.ts\`, \`*.test.tsx\`, \`*.spec.ts\`, or \`*.spec.tsx\`.
- Shared integration or end-to-end suites live in \`tests/\` (e.g. \`tests/db.test.ts\`).

### B. Backend Domain & Use-Case Unit Tests (\`modules/<domain>/use-cases/*.test.ts\`)
Test business logic, calculations, and validation rules in isolation without database dependencies:

\`\`\`typescript
import { describe, it, expect } from "bun:test";
import { calculateNetSalary } from "./calculate-salary.use-case";

describe("Payroll Domain: calculateNetSalary", () => {
  it("calculates net salary after social insurance (10.5%) and tax deductions", () => {
    const result = calculateNetSalary({ grossSalary: 20_000_000, dependents: 1 });
    expect(result.netSalary).toBeGreaterThan(0);
    expect(result.insurance).toBe(2_100_000);
  });

  it("throws HttpError.badRequest if gross salary is negative", () => {
    expect(() => calculateNetSalary({ grossSalary: -5000 })).toThrow();
  });
});
\`\`\`

### C. Server Actions & Database Integration Tests (\`modules/<domain>/presentation/*.test.ts\`)
When running \`com test\` (or \`bun test\`), \`core\` **automatically activates an In-Memory PostgreSQL Database Engine (\`@pglite/core\`)** and auto-loads \`schema.sql\`. You can **import real Server Actions & use-cases directly** without writing manual database mocks! Real PostgreSQL queries, triggers, autoincrement IDs, constraints, and defaults execute in RAM at sub-millisecond speed:

\`\`\`typescript
import { describe, it, expect, beforeEach } from "bun:test";
import { db, truncateTables, HttpError } from "core";
import { createNewsAction, getNewsListAction } from "../index";

describe("News Server Actions (Real In-Memory DB)", () => {
  beforeEach(async () => {
    // Clean up test data before each test run
    await truncateTables(["news"]);
  });

  it("creates a new article in in-memory database with real autoincrement ID", async () => {
    const post = await createNewsAction({
      title: "[TEST] Release v2.0",
      content: "Full changelog details...",
      category: "Announcements",
    });

    expect(post).toBeDefined();
    expect(post.id).toBeTypeOf("number");
    expect(post.title).toBe("[TEST] Release v2.0");

    // Verify it exists in real in-memory query
    const allNews = await getNewsListAction();
    expect(allNews.some((n: any) => n.id === post.id)).toBe(true);
  });

  it("rejects empty title with HttpError.badRequest", async () => {
    expect(createNewsAction({ title: "", content: "test" })).rejects.toThrow("Title is required");
  });
});
\`\`\`

### D. Frontend Feature Hooks & State Tests (\`features/<feature>/hooks/*.test.ts\`)
Test UI state filters, formatting helpers, and custom hooks:

\`\`\`typescript
import { describe, it, expect } from "bun:test";
import { filterNewsByKeyword } from "./useNewsFilter";

describe("Feature News: filterNewsByKeyword", () => {
  const articles = [
    { id: 1, title: "Next.js App Router Guide", category: "Tech" },
    { id: 2, title: "Zalo Mini App Architecture", category: "Mobile" },
  ];

  it("filters articles case-insensitively by title or category", () => {
    const results = filterNewsByKeyword(articles, "zalo");
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe(2);
  });
});
\`\`\`

### E. API Route Handlers Tests (\`app/api/.../route.test.ts\`)
Test Next.js Route Handlers (\`GET\`, \`POST\`, \`PUT\`, \`DELETE\`):

\`\`\`typescript
import { describe, it, expect } from "bun:test";
import { GET } from "./route";

describe("API Route: GET /api/health", () => {
  it("returns HTTP 200 with server status", async () => {
    const req = new Request("http://localhost:3000/api/health");
    const res = await GET(req);
    const data = await res.json();

    expect(res.status).toBe(200);
    expect(data.status).toBe("healthy");
  });
});
\`\`\`

### F. Running Project Tests with \`com test\`
\`\`\`bash
# 1. Run all project tests
com test

# 2. Run tests for a specific domain or file
com test news
com test modules/payroll/use-cases/calculate-salary.use-case.test.ts

# 3. Live TDD Watch Mode (auto re-runs when saving files)
com test --watch

# 4. Filter test cases by description name (regex / substring)
com test --filter "calculateNetSalary"

# 5. Generate Code Coverage Report
com test --coverage

# 6. Stop immediately on first test failure
com test --bail
\`\`\`

---

## 8. AI Coding Commandments for this Codebase

1. **Backend Logic**: ALWAYS put database queries and business logic inside \`modules/<domain>/\`.
2. **Frontend Features**: ALWAYS put feature-specific UI, forms, dialogs, and hooks inside \`features/<feature>/\`.
3. **Shared UI**: Put generic, reusable primitives in \`components/ui/\` (Buttons, Inputs, Modals, Tables).
4. **App Router Pages**: Keep \`app/[route]/page.tsx\` thin; only import and compose features.
5. **Database Access**: Always use \`import { db, sql, HttpError } from "core"\`. Never create ad-hoc database connections.
6. **Testing Mandate**: Whenever you create or modify a module, use-case, action, or hook, ALWAYS write corresponding test cases and verify with \`com test\`.
7. **Hydration Safety**: Guard browser-only APIs (\`window\`, \`document\`, \`localStorage\`) inside \`useEffect\` or \`typeof window !== "undefined"\` checks.
8. **Tailwind CSS**: Use Tailwind utility classes and CSS variables defined in \`app/globals.css\` (\`bg-background\`, \`text-foreground\`, \`bg-card\`, etc.).
`;
}
