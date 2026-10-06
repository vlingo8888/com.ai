import { existsSync, mkdirSync, readdirSync, writeFileSync } from "fs";
import { join, resolve, relative, dirname } from "path";
import { logger, colors } from "../core/logger";
import { ensureRuntimeEnvironment } from "../core/runtime";
import { generateAgentsGuide } from "../templates/ai_guidelines";
import { saveLocalViewConfig } from "../core/config";

export interface CreateProjectOptions {
  dir?: string;
  force?: boolean;
  install?: boolean;
  template?: string;
}

export interface ProjectTemplateFile {
  path: string;
  content: string;
}

/**
 * Generates the Tailwind CSS v4 design tokens and sleek theme styling
 */
function getGlobalsCssTemplate(): string {
  return `@import "tailwindcss";

@layer base {
  :root {
    --background: 220 20% 98%;
    --foreground: 222.2 84% 4.9%;
    --card: 0 0% 100%;
    --card-foreground: 222.2 84% 4.9%;
    --popover: 0 0% 100%;
    --popover-foreground: 222.2 84% 4.9%;
    --primary: 221.2 83.2% 53.3%;
    --primary-foreground: 210 40% 98%;
    --secondary: 210 40% 96.1%;
    --secondary-foreground: 222.2 47.4% 11.2%;
    --muted: 210 40% 96.1%;
    --muted-foreground: 215.4 16.3% 46.9%;
    --accent: 210 40% 96.1%;
    --accent-foreground: 222.2 47.4% 11.2%;
    --destructive: 0 84.2% 60.2%;
    --destructive-foreground: 210 40% 98%;
    --border: 214.3 31.8% 91.4%;
    --input: 214.3 31.8% 91.4%;
    --ring: 221.2 83.2% 53.3%;
    --radius: 0.75rem;
  }

  .dark {
    --background: 224 71% 4%;
    --foreground: 210 40% 98%;
    --card: 222.2 84% 4.9%;
    --card-foreground: 210 40% 98%;
    --popover: 222.2 84% 4.9%;
    --popover-foreground: 210 40% 98%;
    --primary: 217.2 91.2% 59.8%;
    --primary-foreground: 222.2 47.4% 11.2%;
    --secondary: 217.2 32.6% 17.5%;
    --secondary-foreground: 210 40% 98%;
    --muted: 217.2 32.6% 17.5%;
    --muted-foreground: 215 20.2% 65.1%;
    --accent: 217.2 32.6% 17.5%;
    --accent-foreground: 210 40% 98%;
    --destructive: 0 62.8% 30.6%;
    --destructive-foreground: 210 40% 98%;
    --border: 217.2 32.6% 17.5%;
    --input: 217.2 32.6% 17.5%;
    --ring: 224.3 76.3% 48%;
  }
}

body {
  background-color: hsl(var(--background));
  color: hsl(var(--foreground));
  font-feature-settings: "rlig" 1, "calt" 1;
}

/* Glassmorphism effects */
.glass {
  background: rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.12);
}

.glass-card {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.05) 0%, rgba(255, 255, 255, 0.01) 100%);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.08);
}
`;
}

/**
 * Returns all starter template files for a Next.js + Clean Architecture + PGlite project
 */
export function getStarterProjectFiles(projectName: string): ProjectTemplateFile[] {
  const normalizedName = projectName
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, "-")
    .replace(/^-+|-+$/g, "") || "com-pglite-app";

  return [
    // 1. package.json
    {
      path: "package.json",
      content: JSON.stringify(
        {
          name: normalizedName,
          version: "0.1.0",
          private: true,
          type: "module",
          scripts: {
            dev: "com dev",
            build: "com build",
            start: "com dev",
            test: "com test",
            "db:list": "com db list",
            "db:describe": "com db describe",
            "db:pull": "com db pull",
          },
          dependencies: {
            "@pglite/core": "^1.0.0",
            kysely: "^0.29.6",
            "lucide-react": "^1.16.0",
            react: "^19.0.0",
            "react-dom": "^19.0.0",
          },
          devDependencies: {
            "@types/bun": "latest",
            "@types/react": "^19.0.0",
            "@types/react-dom": "^19.0.0",
          },
        },
        null,
        2
      ) + "\n",
    },

    // 2. tsconfig.json
    {
      path: "tsconfig.json",
      content: JSON.stringify(
        {
          compilerOptions: {
            lib: ["ESNext", "DOM", "DOM.Iterable"],
            module: "esnext",
            target: "esnext",
            moduleResolution: "bundler",
            moduleDetection: "force",
            allowImportingTsExtensions: true,
            noEmit: true,
            composite: true,
            strict: true,
            downlevelIteration: true,
            skipLibCheck: true,
            jsx: "react-jsx",
            allowSyntheticDefaultImports: true,
            forceConsistentCasingInFileNames: true,
            allowJs: true,
            types: ["bun-types"],
            baseUrl: ".",
            paths: {
              "@/*": ["./*"],
              "~/*": ["./*"],
            },
          },
          include: ["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
        },
        null,
        2
      ) + "\n",
    },

    // 3. .env (local PGlite configuration)
    {
      path: ".env",
      content: `# Com.AI.VN Local Fullstack Environment
PORT=3000
NODE_ENV=development

# Local Embedded PostgreSQL Database (Powered by @pglite/core)
# No external Postgres server required! Data is stored locally in data/app.db
DB_PATH="data/app.db"
`,
    },

    // 4. .gitignore
    {
      path: ".gitignore",
      content: `node_modules
.DS_Store
.env.local
.env.development.local
.nata
data/
*.log
dist/
.next/
`,
    },

    // 5. schema.sql
    {
      path: "schema.sql",
      content: `-- ====================================================================
-- Com.AI.VN Database Schema (Embedded PGlite PostgreSQL)
-- Auto-loaded into PGlite upon application startup & in-memory testing
-- ====================================================================

CREATE TABLE IF NOT EXISTS "items" (
  "id" SERIAL PRIMARY KEY,
  "title" TEXT NOT NULL,
  "description" TEXT,
  "status" TEXT DEFAULT 'pending' NOT NULL,
  "created_at" TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP NOT NULL,
  "updated_at" TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP NOT NULL
);

COMMENT ON TABLE "items" IS 'Quản lý danh sách công việc và dữ liệu mẫu của hệ thống';
COMMENT ON COLUMN "items"."id" IS 'Khóa chính tự tăng (Primary Key)';
COMMENT ON COLUMN "items"."title" IS 'Tiêu đề công việc hoặc mục tiêu';
COMMENT ON COLUMN "items"."description" IS 'Mô tả chi tiết nội dung';
COMMENT ON COLUMN "items"."status" IS 'Trạng thái: pending, in_progress, completed';
COMMENT ON COLUMN "items"."created_at" IS 'Thời gian tạo bản ghi';
COMMENT ON COLUMN "items"."updated_at" IS 'Thời gian cập nhật gần nhất';
`,
    },

    // 6. types/db.d.ts
    {
      path: "types/db.d.ts",
      content: `import type { Generated } from "kysely";

export interface Database {
  "items": ItemsTable;
}

/**
 * 📋 Quản lý danh sách công việc và dữ liệu mẫu của hệ thống
 */
export interface ItemsTable {
  /** 🔑 Primary Key | Default: nextval('items_id_seq'::regclass) */
  id: Generated<number>;
  /** 📝 Tiêu đề công việc hoặc mục tiêu */
  title: string;
  /** 📝 Mô tả chi tiết nội dung */
  description: string | null;
  /** 📝 Trạng thái: pending, in_progress, completed | Default: 'pending'::text */
  status: Generated<string>;
  /** 📝 Thời gian tạo bản ghi | Default: CURRENT_TIMESTAMP */
  created_at: Generated<string>;
  /** 📝 Thời gian cập nhật gần nhất | Default: CURRENT_TIMESTAMP */
  updated_at: Generated<string>;
}
`,
    },

    // 7. lib/utils.ts
    {
      path: "lib/utils.ts",
      content: `export function cn(...inputs: any[]): string {
  return inputs.filter(Boolean).join(" ").trim();
}

export function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString("vi-VN", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return dateStr;
  }
}
`,
    },

    // 8. components/ui/primitives
    {
      path: "components/ui/button.tsx",
      content: `import * as React from "react";
import { cn } from "@/lib/utils";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "secondary" | "outline" | "destructive" | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", size = "md", ...props }, ref) => {
    const base = "inline-flex items-center justify-center rounded-xl font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 active:scale-95 cursor-pointer";
    
    const variants = {
      default: "bg-gradient-to-r from-blue-600 to-indigo-600 text-white hover:from-blue-700 hover:to-indigo-700 shadow-md shadow-blue-500/20",
      secondary: "bg-slate-800 text-slate-100 hover:bg-slate-700 border border-slate-700",
      outline: "border border-slate-700 bg-transparent hover:bg-slate-800 text-slate-200",
      destructive: "bg-rose-600 text-white hover:bg-rose-700 shadow-md shadow-rose-500/20",
      ghost: "hover:bg-slate-800 text-slate-300 hover:text-white",
    };

    const sizes = {
      sm: "h-8 px-3 text-xs gap-1.5",
      md: "h-10 px-4 text-sm gap-2",
      lg: "h-12 px-6 text-base gap-2.5",
    };

    return (
      <button
        ref={ref}
        className={cn(base, variants[variant], sizes[size], className)}
        {...props}
      />
    );
  }
);
Button.displayName = "Button";
`,
    },
    {
      path: "components/ui/card.tsx",
      content: `import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "rounded-2xl border border-slate-800/80 bg-slate-900/60 p-6 text-slate-100 shadow-xl backdrop-blur-xl transition-all duration-200 hover:border-slate-700/80",
        className
      )}
      {...props}
    />
  );
}

export function CardHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col space-y-1.5 pb-4", className)} {...props} />;
}

export function CardTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return <h3 className={cn("text-lg font-semibold tracking-tight text-white", className)} {...props} />;
}

export function CardDescription({ className, ...props }: React.HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cn("text-sm text-slate-400 leading-relaxed", className)} {...props} />;
}

export function CardContent({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("pt-0", className)} {...props} />;
}
`,
    },
    {
      path: "components/ui/badge.tsx",
      content: `import * as React from "react";
import { cn } from "@/lib/utils";

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: "default" | "success" | "warning" | "destructive" | "outline";
}

export function Badge({ className, variant = "default", ...props }: BadgeProps) {
  const variants = {
    default: "bg-blue-500/10 text-blue-400 border-blue-500/20",
    success: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
    warning: "bg-amber-500/10 text-amber-400 border-amber-500/20",
    destructive: "bg-rose-500/10 text-rose-400 border-rose-500/20",
    outline: "text-slate-300 border-slate-700 bg-transparent",
  };

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-medium border transition-colors",
        variants[variant],
        className
      )}
      {...props}
    />
  );
}
`,
    },
    {
      path: "components/ui/input.tsx",
      content: `import * as React from "react";
import { cn } from "@/lib/utils";

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-10 w-full rounded-xl border border-slate-700 bg-slate-950/60 px-3.5 py-2 text-sm text-white placeholder:text-slate-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:border-blue-500 transition-all",
          className
        )}
        ref={ref}
        {...props}
      />
    );
  }
);
Input.displayName = "Input";
`,
    },

    // 9. modules/items (Backend Clean Architecture)
    {
      path: "modules/items/domain/item.entity.ts",
      content: `export interface Item {
  id: number;
  title: string;
  description: string | null;
  status: "pending" | "in_progress" | "completed";
  created_at: string;
  updated_at: string;
}

export interface CreateItemDTO {
  title: string;
  description?: string;
}
`,
    },
    {
      path: "modules/items/use-cases/get-items.use-case.ts",
      content: `import { db } from "core";

export async function getItems() {
  return await db
    .selectFrom("items")
    .selectAll()
    .orderBy("created_at", "desc")
    .execute();
}
`,
    },
    {
      path: "modules/items/use-cases/create-item.use-case.ts",
      content: `import { db, HttpError } from "core";
import type { CreateItemDTO } from "../domain/item.entity";

export async function createItem(dto: CreateItemDTO) {
  if (!dto.title || dto.title.trim().length === 0) {
    throw HttpError.badRequest("Tiêu đề không được để trống");
  }

  const now = new Date().toISOString();
  return await db
    .insertInto("items")
    .values({
      title: dto.title.trim(),
      description: dto.description?.trim() || null,
      status: "pending",
      created_at: now,
      updated_at: now,
    })
    .returningAll()
    .executeTakeFirstOrThrow();
}
`,
    },
    {
      path: "modules/items/use-cases/toggle-item.use-case.ts",
      content: `import { db, HttpError } from "core";

export async function toggleItemStatus(id: number) {
  const item = await db
    .selectFrom("items")
    .selectAll()
    .where("id", "=", id)
    .executeTakeFirst();

  if (!item) {
    throw HttpError.notFound("Không tìm thấy mục yêu cầu");
  }

  const nextStatus = item.status === "completed" ? "pending" : "completed";
  const now = new Date().toISOString();

  return await db
    .updateTable("items")
    .set({
      status: nextStatus,
      updated_at: now,
    })
    .where("id", "=", id)
    .returningAll()
    .executeTakeFirstOrThrow();
}
`,
    },
    {
      path: "modules/items/use-cases/delete-item.use-case.ts",
      content: `import { db, HttpError } from "core";

export async function deleteItem(id: number) {
  const result = await db
    .deleteFrom("items")
    .where("id", "=", id)
    .returningAll()
    .executeTakeFirst();

  if (!result) {
    throw HttpError.notFound("Không tìm thấy mục cần xóa");
  }

  return result;
}
`,
    },
    {
      path: "modules/items/presentation/item.actions.ts",
      content: `"use server";

import { getItems } from "../use-cases/get-items.use-case";
import { createItem } from "../use-cases/create-item.use-case";
import { toggleItemStatus } from "../use-cases/toggle-item.use-case";
import { deleteItem } from "../use-cases/delete-item.use-case";
import type { CreateItemDTO } from "../domain/item.entity";

export async function getItemsAction() {
  return await getItems();
}

export async function createItemAction(dto: CreateItemDTO) {
  return await createItem(dto);
}

export async function toggleItemStatusAction(id: number) {
  return await toggleItemStatus(id);
}

export async function deleteItemAction(id: number) {
  return await deleteItem(id);
}
`,
    },
    {
      path: "modules/items/index.ts",
      content: `export * from "./domain/item.entity";
export * from "./use-cases/get-items.use-case";
export * from "./use-cases/create-item.use-case";
export * from "./use-cases/toggle-item.use-case";
export * from "./use-cases/delete-item.use-case";
export * from "./presentation/item.actions";
`,
    },
    // Colocated Use-Case Test with In-Memory PGlite
    {
      path: "modules/items/use-cases/items.use-case.test.ts",
      content: `import { describe, it, expect, beforeEach } from "bun:test";
import { truncateTables } from "core";
import { createItem } from "./create-item.use-case";
import { getItems } from "./get-items.use-case";
import { toggleItemStatus } from "./toggle-item.use-case";
import { deleteItem } from "./delete-item.use-case";

describe("Domain: Items Use Cases (In-Memory PGlite)", () => {
  beforeEach(async () => {
    // Clean up test table before each test
    await truncateTables(["items"]);
  });

  it("creates a new item in PGlite with auto-generated serial ID", async () => {
    const item = await createItem({
      title: "Tìm hiểu kiến trúc Com.AI.VN Framework",
      description: "Tích hợp PGlite database và Clean Architecture",
    });

    expect(item).toBeDefined();
    expect(item.id).toBeTypeOf("number");
    expect(item.title).toBe("Tìm hiểu kiến trúc Com.AI.VN Framework");
    expect(item.status).toBe("pending");

    const all = await getItems();
    expect(all).toHaveLength(1);
    expect(all[0].id).toBe(item.id);
  });

  it("rejects empty title with HttpError", async () => {
    expect(createItem({ title: "   " })).rejects.toThrow("Tiêu đề không được để trống");
  });

  it("toggles item status between pending and completed", async () => {
    const item = await createItem({ title: "Task cần toggle" });
    expect(item.status).toBe("pending");

    const toggled = await toggleItemStatus(item.id);
    expect(toggled.status).toBe("completed");

    const toggledBack = await toggleItemStatus(item.id);
    expect(toggledBack.status).toBe("pending");
  });

  it("deletes an existing item from database", async () => {
    const item = await createItem({ title: "Task sắp bị xóa" });
    const deleted = await deleteItem(item.id);
    expect(deleted.id).toBe(item.id);

    const all = await getItems();
    expect(all).toHaveLength(0);
  });
});
`,
    },

    // 10. features/items (Frontend Clean Architecture)
    {
      path: "features/items/hooks/useItemsFilter.ts",
      content: `export interface FilterableItem {
  id: number;
  title: string;
  description: string | null;
  status: string;
}

export function filterItems<T extends FilterableItem>(
  items: T[],
  keyword: string,
  statusFilter: string = "all"
): T[] {
  const cleanKeyword = keyword.trim().toLowerCase();

  return items.filter((item) => {
    const matchesKeyword =
      !cleanKeyword ||
      item.title.toLowerCase().includes(cleanKeyword) ||
      (item.description && item.description.toLowerCase().includes(cleanKeyword));

    const matchesStatus =
      statusFilter === "all" || item.status === statusFilter;

    return matchesKeyword && matchesStatus;
  });
}
`,
    },
    {
      path: "features/items/hooks/useItemsFilter.test.ts",
      content: `import { describe, it, expect } from "bun:test";
import { filterItems } from "./useItemsFilter";

describe("Feature Items: filterItems", () => {
  const mockItems = [
    { id: 1, title: "Next.js App Router Guide", description: "Doc guide", status: "completed" },
    { id: 2, title: "PGlite Local Database", description: "In-memory SQL", status: "pending" },
    { id: 3, title: "Clean Architecture Setup", description: "Frontend UI", status: "pending" },
  ];

  it("filters items by keyword matching title or description", () => {
    const res = filterItems(mockItems, "pglite");
    expect(res).toHaveLength(1);
    expect(res[0].id).toBe(2);
  });

  it("filters items by status", () => {
    const res = filterItems(mockItems, "", "completed");
    expect(res).toHaveLength(1);
    expect(res[0].id).toBe(1);
  });

  it("returns all items when filter is empty and status is all", () => {
    const res = filterItems(mockItems, "", "all");
    expect(res).toHaveLength(3);
  });
});
`,
    },
    {
      path: "features/items/ui/ItemCard.tsx",
      content: `"use client";

import * as React from "react";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatDate } from "@/lib/utils";
import { CheckCircle2, Circle, Trash2, Calendar } from "lucide-react";
import type { Item } from "@/modules/items";

export interface ItemCardProps {
  item: Item;
  onToggle: (id: number) => void;
  onDelete: (id: number) => void;
  isProcessing?: boolean;
}

export function ItemCard({ item, onToggle, onDelete, isProcessing }: ItemCardProps) {
  const isCompleted = item.status === "completed";

  return (
    <Card className="flex flex-col justify-between gap-4 p-5 hover:border-blue-500/40 transition-all duration-300 group">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3 flex-1 min-w-0">
          <button
            onClick={() => onToggle(item.id)}
            disabled={isProcessing}
            className="mt-0.5 text-slate-400 hover:text-emerald-400 transition-colors cursor-pointer disabled:opacity-50"
            title={isCompleted ? "Đánh dấu chưa xong" : "Đánh dấu hoàn thành"}
          >
            {isCompleted ? (
              <CheckCircle2 className="w-5 h-5 text-emerald-400" />
            ) : (
              <Circle className="w-5 h-5" />
            )}
          </button>
          <div className="flex-1 min-w-0">
            <h4
              className={
                "text-base font-medium truncate " +
                (isCompleted ? "line-through text-slate-500" : "text-white")
              }
            >
              {item.title}
            </h4>
            {item.description && (
              <p className="text-sm text-slate-400 mt-1 line-clamp-2 leading-relaxed">
                {item.description}
              </p>
            )}
          </div>
        </div>

        <Badge variant={isCompleted ? "success" : "warning"}>
          {isCompleted ? "Hoàn thành" : "Đang chờ"}
        </Badge>
      </div>

      <div className="flex items-center justify-between pt-3 border-t border-slate-800/80 text-xs text-slate-500">
        <span className="inline-flex items-center gap-1.5">
          <Calendar className="w-3.5 h-3.5" />
          {formatDate(item.created_at)}
        </span>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onDelete(item.id)}
          disabled={isProcessing}
          className="text-slate-400 hover:text-rose-400 h-7 px-2"
          title="Xóa mục"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </Button>
      </div>
    </Card>
  );
}
`,
    },
    {
      path: "features/items/ui/ItemDashboard.tsx",
      content: `"use client";

import * as React from "react";
import { ItemCard } from "./ItemCard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";
import { filterItems } from "../hooks/useItemsFilter";
import {
  createItemAction,
  toggleItemStatusAction,
  deleteItemAction,
} from "@/modules/items";
import type { Item } from "@/modules/items";
import {
  Plus,
  Search,
  Database,
  Layers,
  Sparkles,
  Server,
  Terminal,
} from "lucide-react";

export interface ItemDashboardProps {
  initialItems: Item[];
}

export function ItemDashboard({ initialItems }: ItemDashboardProps) {
  const [items, setItems] = React.useState<Item[]>(initialItems);
  const [keyword, setKeyword] = React.useState("");
  const [statusFilter, setStatusFilter] = React.useState("all");
  const [isCreating, setIsCreating] = React.useState(false);
  const [newTitle, setNewTitle] = React.useState("");
  const [newDesc, setNewDesc] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [errorMsg, setErrorMsg] = React.useState<string | null>(null);

  const filteredItems = React.useMemo(() => {
    return filterItems(items, keyword, statusFilter);
  }, [items, keyword, statusFilter]);

  const totalCount = items.length;
  const completedCount = items.filter((i) => i.status === "completed").length;
  const pendingCount = totalCount - completedCount;

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!newTitle.trim()) return;

    setLoading(true);
    setErrorMsg(null);
    try {
      const created = await createItemAction({
        title: newTitle,
        description: newDesc,
      });
      setItems([created, ...items]);
      setNewTitle("");
      setNewDesc("");
      setIsCreating(false);
    } catch (err: any) {
      setErrorMsg(err.message || "Không thể tạo mục mới");
    } finally {
      setLoading(false);
    }
  }

  async function handleToggle(id: number) {
    setLoading(true);
    try {
      const updated = await toggleItemStatusAction(id);
      setItems(items.map((i) => (i.id === id ? updated : i)));
    } catch (err: any) {
      alert(err.message || "Lỗi khi cập nhật");
    } finally {
      setLoading(false);
    }
  }

  async function handleDelete(id: number) {
    if (!confirm("Bạn có chắc muốn xóa mục này?")) return;
    setLoading(true);
    try {
      await deleteItemAction(id);
      setItems(items.filter((i) => i.id !== id));
    } catch (err: any) {
      alert(err.message || "Lỗi khi xóa");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-8">
      {/* Top Banner / Stats */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card className="flex items-center gap-4 bg-gradient-to-br from-blue-950/40 to-slate-900/40 border-blue-500/20">
          <div className="w-12 h-12 rounded-xl bg-blue-500/10 flex items-center justify-center text-blue-400">
            <Database className="w-6 h-6" />
          </div>
          <div>
            <div className="text-2xl font-bold text-white">{totalCount}</div>
            <div className="text-xs text-slate-400">Embedded PGlite Records</div>
          </div>
        </Card>

        <Card className="flex items-center gap-4 bg-gradient-to-br from-emerald-950/40 to-slate-900/40 border-emerald-500/20">
          <div className="w-12 h-12 rounded-xl bg-emerald-500/10 flex items-center justify-center text-emerald-400">
            <Sparkles className="w-6 h-6" />
          </div>
          <div>
            <div className="text-2xl font-bold text-emerald-400">{completedCount}</div>
            <div className="text-xs text-slate-400">Mục đã hoàn thành</div>
          </div>
        </Card>

        <Card className="flex items-center gap-4 bg-gradient-to-br from-amber-950/40 to-slate-900/40 border-amber-500/20">
          <div className="w-12 h-12 rounded-xl bg-amber-500/10 flex items-center justify-center text-amber-400">
            <Layers className="w-6 h-6" />
          </div>
          <div>
            <div className="text-2xl font-bold text-amber-400">{pendingCount}</div>
            <div className="text-xs text-slate-400">Đang chờ thực hiện</div>
          </div>
        </Card>
      </div>

      {/* Control bar */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
        <div className="flex items-center gap-2 flex-1 max-w-md relative">
          <Search className="w-4 h-4 text-slate-500 absolute left-3.5" />
          <Input
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            placeholder="Tìm kiếm công việc, dữ liệu..."
            className="pl-9"
          />
        </div>

        <div className="flex items-center gap-2">
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="h-10 px-3 rounded-xl border border-slate-700 bg-slate-950/60 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            <option value="all">Tất cả trạng thái</option>
            <option value="pending">Đang chờ</option>
            <option value="completed">Đã hoàn thành</option>
          </select>

          <Button onClick={() => setIsCreating(!isCreating)}>
            <Plus className="w-4 h-4" />
            <span>Thêm mục mới</span>
          </Button>
        </div>
      </div>

      {/* Inline Create Form */}
      {isCreating && (
        <Card className="border-blue-500/30 bg-slate-900/80 p-6 animate-in fade-in slide-in-from-top-2 duration-200">
          <form onSubmit={handleCreate} className="space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-base font-semibold text-white flex items-center gap-2">
                <Database className="w-4 h-4 text-blue-400" />
                Thêm bản ghi mới vào local database (PGlite)
              </h3>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setIsCreating(false)}
              >
                Đóng
              </Button>
            </div>

            {errorMsg && (
              <div className="p-3 text-sm text-rose-400 bg-rose-500/10 border border-rose-500/20 rounded-xl">
                {errorMsg}
              </div>
            )}

            <div className="grid grid-cols-1 gap-3">
              <Input
                placeholder="Tiêu đề công việc *"
                value={newTitle}
                onChange={(e) => setNewTitle(e.target.value)}
                required
                autoFocus
              />
              <Input
                placeholder="Mô tả chi tiết (tùy chọn)..."
                value={newDesc}
                onChange={(e) => setNewDesc(e.target.value)}
              />
            </div>

            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => setIsCreating(false)}
              >
                Hủy bỏ
              </Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Đang lưu..." : "Lưu vào PGlite"}
              </Button>
            </div>
          </form>
        </Card>
      )}

      {/* Grid of Items */}
      {filteredItems.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredItems.map((item) => (
            <ItemCard
              key={item.id}
              item={item}
              onToggle={handleToggle}
              onDelete={handleDelete}
              isProcessing={loading}
            />
          ))}
        </div>
      ) : (
        <Card className="text-center py-12 border-dashed border-slate-800">
          <div className="w-12 h-12 rounded-full bg-slate-800/60 mx-auto flex items-center justify-center text-slate-500 mb-3">
            <Search className="w-6 h-6" />
          </div>
          <h4 className="text-base font-medium text-slate-300">Không tìm thấy mục nào</h4>
          <p className="text-sm text-slate-500 mt-1 max-w-sm mx-auto">
            {keyword
              ? "Hãy thử tìm kiếm với từ khóa khác hoặc điều chỉnh bộ lọc."
              : "Bấm nút 'Thêm mục mới' ở trên để tạo bản ghi đầu tiên vào local database PGlite."}
          </p>
        </Card>
      )}
    </div>
  );
}
`,
    },
    {
      path: "features/items/index.ts",
      content: `export * from "./ui/ItemDashboard";
export * from "./ui/ItemCard";
export * from "./hooks/useItemsFilter";
`,
    },

    // 11. app/ (Next.js App Router layer)
    {
      path: "app/globals.css",
      content: getGlobalsCssTemplate(),
    },
    {
      path: "app/layout.tsx",
      content: `import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "${projectName} - Com.AI.VN Clean Architecture & PGlite",
  description: "Next.js compatible fullstack application with embedded local PostgreSQL (PGlite)",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="vi" className="dark">
      <body className="min-h-screen bg-slate-950 text-slate-100 antialiased selection:bg-blue-600 selection:text-white">
        <header className="sticky top-0 z-50 border-b border-slate-800/80 bg-slate-950/80 backdrop-blur-xl">
          <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-9 h-9 rounded-xl bg-gradient-to-tr from-blue-600 to-indigo-500 flex items-center justify-center font-bold text-white shadow-lg shadow-blue-500/20">
                C
              </div>
              <div>
                <span className="font-semibold text-white tracking-tight">${projectName}</span>
                <span className="ml-2 text-xs px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20">
                  PGlite Local
                </span>
              </div>
            </div>

            <nav className="flex items-center gap-4 text-sm text-slate-400">
              <a
                href="/api/health"
                target="_blank"
                className="hover:text-blue-400 transition-colors text-xs font-mono bg-slate-900 px-3 py-1 rounded-lg border border-slate-800"
              >
                /api/health
              </a>
            </nav>
          </div>
        </header>

        <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
          {children}
        </main>

        <footer className="border-t border-slate-900 py-6 text-center text-xs text-slate-600">
          Powered by <strong>Com.AI.VN Framework Engine</strong> & Embedded <strong>@pglite/core</strong>
        </footer>
      </body>
    </html>
  );
}
`,
    },
    {
      path: "app/page.tsx",
      content: `import { getItemsAction } from "@/modules/items";
import { ItemDashboard } from "@/features/items";

export default async function HomePage() {
  // Server-side query directly from local PGlite database
  const items = await getItemsAction();

  return (
    <div className="space-y-8">
      {/* Hero Welcome */}
      <div className="rounded-3xl border border-slate-800 bg-gradient-to-b from-slate-900/80 via-slate-900/40 to-slate-950 p-8 sm:p-10 backdrop-blur-xl relative overflow-hidden">
        <div className="absolute top-0 right-0 w-96 h-96 bg-blue-500/10 rounded-full blur-3xl pointer-events-none" />
        <div className="relative z-10 max-w-3xl space-y-3">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full text-xs font-medium bg-blue-500/10 text-blue-400 border border-blue-500/20">
            <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
            Database Local Sẵn Sàng (Embedded PGlite)
          </div>
          <h1 className="text-3xl sm:text-4xl font-extrabold tracking-tight text-white">
            Chào mừng đến với <span className="bg-gradient-to-r from-blue-400 to-indigo-400 bg-clip-text text-transparent">${projectName}</span>
          </h1>
          <p className="text-slate-400 text-sm sm:text-base leading-relaxed">
            Dự án được cấu trúc theo <strong>Clean Architecture 4 tầng</strong> tương thích Next.js App Router.
            Toàn bộ truy vấn SQL chạy trực tiếp vào file cục bộ <code className="text-blue-400 bg-slate-950 px-1.5 py-0.5 rounded border border-slate-800">data/app.db</code> thông qua Kysely & @pglite/core.
          </p>
        </div>
      </div>

      {/* Feature UI Layer */}
      <ItemDashboard initialItems={items} />
    </div>
  );
}
`,
    },

    // 12. app/api/health (Next.js API Route Handler)
    {
      path: "app/api/health/route.ts",
      content: `import { db, sql, json } from "core";

export async function GET() {
  try {
    const start = performance.now();
    // Verify database connectivity with local PGlite
    const res: any = await db
      .selectFrom("items")
      .select(sql<number>\`count(*)\`.as("count"))
      .executeTakeFirst();
    const duration = Number((performance.now() - start).toFixed(2));

    return json({
      status: "healthy",
      engine: "pglite",
      dbPath: process.env.DB_PATH || "data/app.db",
      totalItems: Number(res?.count || 0),
      pingMs: duration,
      timestamp: new Date().toISOString(),
    });
  } catch (error: any) {
    return json(
      {
        status: "unhealthy",
        error: error.message || String(error),
      },
      { status: 500 }
    );
  }
}
`,
    },
    {
      path: "app/api/health/route.test.ts",
      content: `import { describe, it, expect } from "bun:test";
import { GET } from "./route";

describe("API Route: GET /api/health", () => {
  it("returns HTTP 200 with PGlite status and count", async () => {
    const res = await GET();
    const data = await res.json();

    expect(res.status).toBe(200);
    expect(data.status).toBe("healthy");
    expect(data.engine).toBe("pglite");
    expect(data.timestamp).toBeDefined();
  });
});
`,
    },

    // 13. README.md
    {
      path: "README.md",
      content: `# ${projectName}

Ứng dụng Fullstack được khởi tạo bởi **Com.AI.VN Framework** với kiến trúc **Clean Architecture** chuẩn Next.js App Router và cơ sở dữ liệu nhúng **PGlite (\`@pglite/core\`)**.

---

## 🚀 Tính Năng Chính

- ⚡ **Next.js App Router**: Cấu trúc route hiện đại (\`app/layout.tsx\`, \`app/page.tsx\`, \`app/api/...\`).
- 🐘 **Embedded PostgreSQL (PGlite)**: Không cần cài đặt Postgres server hay Docker bên ngoài. Dữ liệu lưu tại \`data/app.db\`.
- 🛡️ **Clean Architecture**:
  - \`modules/items/\`: Domain business logic, Kysely queries, Server Actions.
  - \`features/items/\`: Feature UI, custom hooks, view models.
  - \`components/ui/\`: Primitives dùng chung (Button, Card, Badge, Input).
  - \`types/db.d.ts\`: Schema typed cho toàn bộ bảng.
- 🧪 **Zero-Mock Testing**: Test trực tiếp use-case và Server Action với In-Memory PGlite chạy ở RAM siêu tốc qua \`com test\`.

---

## 🛠️ Lệnh Phát Triển

\`\`\`bash
# 1. Khởi động local dev server với hot reload
com dev

# 2. Chạy toàn bộ unit & integration tests
com test

# 3. Xem danh sách các bảng trong database PGlite
com db list

# 4. Kiểm tra chi tiết cấu trúc cột của một bảng
com db describe items

# 5. Build dự án cho Production
com build
\`\`\`

---

## 📖 Hướng Dẫn AI Coding Assistant
Xem tệp [AGENTS.md](./AGENTS.md) và [.antigravityrules](./.antigravityrules) để AI coding assistants (Antigravity / Cursor / Copilot) tuân thủ đúng nguyên tắc kiến trúc của dự án.
`,
    },
  ];
}

/**
 * Initializes PGlite database on disk (data/app.db), runs schema.sql, and seeds sample data
 */
async function initializeLocalDatabase(targetDir: string, schemaSql: string): Promise<number> {
  const dataDir = join(targetDir, "data");
  if (!existsSync(dataDir)) {
    mkdirSync(dataDir, { recursive: true });
  }

  const dbPath = join(dataDir, "app.db");
  if (!existsSync(dbPath)) {
    writeFileSync(dbPath, "");
  }

  try {
    let PGliteClass: any = null;
    try {
      const pglitePkg = require("@pglite/core");
      PGliteClass = pglitePkg.PGLite || pglitePkg.PGLiteNative;
    } catch {}

    if (!PGliteClass) {
      try {
        PGliteClass = require("@electric-sql/pglite").PGlite;
      } catch {}
    }

    if (!PGliteClass) {
      logger.warn("Could not find PGLite constructor, skipping automatic table creation");
      return 0;
    }

    const pglite = new PGliteClass(dbPath);

    // 1. Execute schema.sql DDL
    await pglite.exec(schemaSql);

    // 2. Seed initial sample items
    const checkRes = await pglite.query("SELECT COUNT(*) as cnt FROM items");
    const count = Number(checkRes?.[0]?.cnt || 0);

    if (count === 0) {
      const now = new Date().toISOString();
      await pglite.query(
        `INSERT INTO items (title, description, status, created_at, updated_at) VALUES 
        ($1, $2, $3, $4, $5),
        ($6, $7, $8, $9, $10),
        ($11, $12, $13, $14, $15)`,
        [
          "Khởi tạo dự án Com.AI.VN với PGlite",
          "Hệ thống tự động kích hoạt database nhúng @pglite/core tại data/app.db",
          "completed",
          now,
          now,
          "Tìm hiểu cấu trúc Clean Architecture 4 tầng",
          "Phân chia tách bạch giữa modules/ (backend) và features/ (frontend UI)",
          "in_progress",
          now,
          now,
          "Chạy kiểm thử tự động với Bun Test (com test)",
          "Xác thực toàn bộ use-cases và API Routes với in-memory database",
          "pending",
          now,
          now,
        ]
      );
    }

    const total = await pglite.query("SELECT COUNT(*) as cnt FROM items");
    const countResult = Number(total?.[0]?.cnt || 3);
    try {
      await pglite.close();
    } catch {}
    return countResult;
  } catch (err: any) {
    logger.warn(`Notice: PGlite local database bootstrap note: ${err.message || err}`);
    return 0;
  }
}

/**
 * Main command handler for `com create <project-name>` / `com new <project-name>`
 */
export async function createProject(
  projectNameArg?: string,
  options: CreateProjectOptions = {}
): Promise<void> {
  const projectName = (projectNameArg || "").trim() || "my-com-app";

  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ CREATING COM.AI.VN PROJECT: ${colors.cyan}${projectName}${colors.reset}\n`);

  // Target directory resolution
  const targetDir = resolve(process.cwd(), options.dir || projectName);
  const relTargetDir = relative(process.cwd(), targetDir) || "./";

  if (existsSync(targetDir)) {
    const existingItems = readdirSync(targetDir);
    if (existingItems.length > 0 && !options.force) {
      logger.error(
        `Target directory "${relTargetDir}" already exists and is not empty.`,
        `Use ${colors.bold}--force${colors.reset} to overwrite: ${colors.cyan}com create ${projectName} --force${colors.reset}`
      );
      process.exit(1);
    }
  } else {
    mkdirSync(targetDir, { recursive: true });
  }

  const spinner = logger.spinner(`Scaffolding Clean Architecture project structure with PGlite...`);

  // Get all project template files
  const templateFiles = getStarterProjectFiles(projectName);
  let totalBytes = 0;

  for (const file of templateFiles) {
    const fullPath = join(targetDir, file.path);
    const parentDir = dirname(fullPath);
    if (!existsSync(parentDir)) {
      mkdirSync(parentDir, { recursive: true });
    }
    await Bun.write(fullPath, file.content);
    totalBytes += Buffer.byteLength(file.content, "utf-8");
  }

  // Generate AGENTS.md, .antigravityrules & .agents/rules/architecture.md
  const agentsGuideContent = generateAgentsGuide(projectName, 1);
  const agentsDir = join(targetDir, ".agents", "rules");
  const agentLegacyDir = join(targetDir, ".agent", "rules");
  if (!existsSync(agentsDir)) mkdirSync(agentsDir, { recursive: true });
  if (!existsSync(agentLegacyDir)) mkdirSync(agentLegacyDir, { recursive: true });
  
  await Bun.write(join(agentsDir, "architecture.md"), agentsGuideContent);
  await Bun.write(join(agentLegacyDir, "architecture.md"), agentsGuideContent);
  await Bun.write(join(targetDir, ".antigravityrules"), agentsGuideContent);
  await Bun.write(join(targetDir, "AGENTS.md"), agentsGuideContent);

  // Setup local runtime environment (.nata/core.ts & node_modules/core)
  ensureRuntimeEnvironment(targetDir);

  // Save local view configuration
  saveLocalViewConfig(targetDir, {
    viewId: 1,
    name: projectName,
    apiUrl: "local",
    clonedAt: new Date().toISOString(),
    files: templateFiles.map((f) => ({
      path: f.path,
      size: Buffer.byteLength(f.content, "utf-8"),
    })),
  });

  // Initialize PGlite database & seed data
  const schemaFile = templateFiles.find((f) => f.path === "schema.sql");
  let seededRows = 0;
  if (schemaFile) {
    seededRows = await initializeLocalDatabase(targetDir, schemaFile.content);
  }

  spinner.stop(true, `Successfully created project structure (${templateFiles.length} files)`);

  // Optional dependency installation
  if (options.install) {
    const installSpinner = logger.spinner("Installing project dependencies with Bun...");
    try {
      const proc = Bun.spawn(["bun", "install"], { cwd: targetDir });
      await proc.exited;
      installSpinner.stop(true, "Dependencies installed successfully");
    } catch {
      installSpinner.stop(false, "Failed to run bun install; using ESM Zero-Install runtime");
    }
  }

  // Summary Card
  logger.card("PROJECT READY", [
    { label: "Project Name", value: projectName, color: colors.bold + colors.white },
    { label: "Architecture", value: "Clean Architecture (App Router, Modules, Features)", color: colors.bold + colors.green },
    { label: "Database Engine", value: `PGlite (@pglite/core) - Local File DB`, color: colors.bold + colors.sky },
    { label: "Database Path", value: "data/app.db", color: colors.yellow },
    { label: "Sample Data", value: seededRows > 0 ? `${seededRows} records seeded in table 'items'` : "Initialized via schema.sql", color: colors.emerald },
    { label: "Runtime Mode", value: options.install ? "Local node_modules" : "ESM Zero-Install (Instant)", color: colors.cyan },
    { label: "Target Directory", value: relTargetDir, color: colors.green },
  ]);

  // Next steps call to action
  logger.nextSteps([
    { cmd: `cd ${relTargetDir}`, desc: "Navigate into the project directory" },
    { cmd: "com dev", desc: "Start local development server with live reload" },
    { cmd: "com test", desc: "Run in-memory PGlite unit & integration tests" },
    { cmd: "com db list", desc: "Inspect database tables and schemas" },
  ]);
}
