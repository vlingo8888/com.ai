# 🗺️ Roadmap & Feature Gap Analysis vs Next.js (App Router)

> **Project:** `com.ai.vn-framework`  
> **Goal:** High-Performance, Zero-Install Rust/Bun Fullstack Framework compatible with Next.js App Router conventions.  
> **Last Updated:** 2026-09-17

---

## 📊 Feature Status Overview

| Feature Category | Next.js 14/15 Spec | `com.ai.vn` Status | Priority |
| :--- | :--- | :---: | :---: |
| **App Routing Core** | `page.tsx`, `layout.tsx`, `[id]` | ✅ **Fully Supported** | Completed |
| **Database & Migration Engine** | Kysely, Postgres Introspection, `com migrate`, `com db *` | ✅ **Fully Supported** | Completed |
| **Tailwind CSS v4 & Theming** | Native JIT, CSS variable theming | ✅ **Fully Supported** | Completed |
| **ESM Zero-Install (Client)** | Remote CDN dynamic resolution | ✅ **Fully Supported** | Completed |
| **Route Special Files** | `loading.tsx`, `error.tsx`, `not-found.tsx`, `template.tsx` | ✅ **Fully Supported** | Completed |
| **Next.js Shims & APIs** | `next/navigation`, `next/headers`, `next/link`, `next/image` | 🟡 **Partial** | **High (P0)** |
| **SEO & Metadata API** | `export const metadata`, `generateMetadata()`, `sitemap.ts` | 🔴 **Missing** | **High (P0)** |
| **Route Handlers (API Routes)** | `app/api/.../route.ts` (`GET`, `POST`, `PUT`, `DELETE`) | 🔴 **Missing** | **High (P0)** |
| **Middleware Engine** | `middleware.ts` with matcher & cookie/auth rewrites | 🔴 **Missing** | **Medium (P1)** |
| **Fonts & Media Optimization** | `next/font/google`, `next/font/local`, dynamic `next/image` resize | 🔴 **Missing** | **Medium (P1)** |
| **Data Cache & Revalidation** | `revalidatePath()`, `revalidateTag()`, `fetch(..., { next })` | 🔴 **Missing** | **Medium (P1)** |
| **React Server Components (RSC)** | Streaming HTML, `<Suspense>`, partial prerendering | 🔴 **Missing** | **Long-term (P2)** |

---

## 1. 📁 App Router & Special Files (Priority: P0)

Next.js provides convention-based file names within route folders to handle UI states declaratively:

- [x] **`loading.tsx` Support**:
  - Automatically wrap `page.tsx` with React `<Suspense fallback={<Loading />}>` when `loading.tsx` is present in the route folder or parent folders.
- [x] **`error.tsx` & `global-error.tsx` Support**:
  - Automatically wrap route boundaries with a React Error Boundary component rendering `error.tsx` (with `reset()` prop).
- [x] **`not-found.tsx` Support**:
  - Render `not-found.tsx` when `notFound()` is triggered from `next/navigation` or a route doesn't match.
- [x] **`template.tsx` Support**:
  - Similar to `layout.tsx`, but creates a new instance for each child on navigation (does not preserve state, re-triggers animations).
- [x] **Advanced Dynamic Routing**:
  - [x] Catch-all routes: `app/docs/[...slug]/page.tsx`
  - [x] Optional catch-all routes: `app/shop/[[...slug]]/page.tsx`
  - [x] Route Groups: `app/(marketing)/...` vs `app/(dashboard)/...` (folders with parentheses should not affect URL pathname).
- [ ] **Parallel Routes & Intercepting Routes (P2)**:
  - `@modal/page.tsx` slots and `(..)photos/[id]` modal interception.

---

## 2. ⚡ Route Handlers / Custom API Endpoints (Priority: P0)

Next.js allows defining API endpoints via `route.ts` / `route.js`:

- [x] **`app/**/route.ts` Handler Engine**:
  - [x] Detect `route.ts` in any `app/` folder (e.g. `app/api/webhooks/vnpay/route.ts`, `app/api/auth/login/route.ts`).
  - [x] Support exported HTTP method functions: `export async function GET(request: Request) {}`, `POST`, `PUT`, `DELETE`, `PATCH`, `OPTIONS`, `HEAD`.
  - [x] Provide `NextRequest` and `NextResponse.json({ ... }, { status: 200 })` helpers.
  - [x] Support dynamic params in route handlers (e.g. `app/api/users/[id]/route.ts`).

---

## 3. 🌐 Metadata & SEO Engine (Priority: P0)

Next.js has a built-in Metadata API to define HTML `<head>` tags:

- [x] **Static Metadata Object**:
  - [x] Extract `export const metadata: Metadata = { title: "...", description: "...", openGraph: { ... }, icons: "..." }` from `layout.tsx` and `page.tsx`.
  - [x] Title templates (`%s | Brand`, `default`, `absolute`) and hierarchical cascade merging.
  - [x] Inject `<title>`, `<meta name="description">`, `<meta property="og:*">`, `<link rel="canonical">`, twitter, theme-color, robots into the server-rendered HTML shell.
  - [x] Dynamic client-side SPA navigation `<head>` updating (`updateClientMetadata`).
- [x] **Dynamic `generateMetadata()`**:
  - [x] Support `export async function generateMetadata({ params, searchParams }, parent): Promise<Metadata>`.
- [x] **Special Metadata Files**:
  - [x] `app/robots.ts` -> `/robots.txt` (standard robots text output).
  - [x] `app/sitemap.ts` -> `/sitemap.xml` (valid Sitemaps 0.9 XML output).
  - [x] `app/manifest.ts` -> `/manifest.webmanifest` / `/manifest.json` (PWA / Web App manifest JSON).
  - [x] `app/icon.png` / `app/apple-icon.png` / `app/opengraph-image.png` / `app/twitter-image.png` (automatic route asset serving).
  - [x] TypeScript declarations for `Metadata`, `ResolvingMetadata`, `MetadataRoute` in `next` module shims.

---

## 4. 🧩 Next.js Package Shims & Extensions (Priority: P0 - P1)

Improve existing shims to provide 100% drop-in compatibility for common packages:

- [ ] **`next/font` (P0)**:
  - `next/font/google`: Support `import { Inter, Plus_Jakarta_Sans } from "next/font/google"`.
  - Automatically load Google Fonts via CSS CDN or local preload without breaking build.
  - `next/font/local`: Support local `.woff2` font definitions.
- [ ] **`next/image` Enhancement (P1)**:
  - Add native image optimization / resizing endpoint (`/_nata/image?url=...&w=640&q=75`).
  - Convert to WebP / AVIF format dynamically.
- [ ] **`next/dynamic` (P1)**:
  - Support `dynamic(() => import("./Component"), { ssr: false, loading: () => <Spinner /> })`.
- [ ] **`next/script` (P1)**:
  - Support `<Script src="..." strategy="afterInteractive" />`.
- [ ] **`next/cache` (P1)**:
  - Implement `revalidatePath(path)` and `revalidateTag(tag)` shims to invalidate client/server caches.

---

## 5. 🛡️ Middleware Engine (Priority: P1)

Next.js allows running code before a request is completed:

- [ ] **`middleware.ts` Support**:
  - Detect root `middleware.ts` or `src/middleware.ts`.
  - Support `export function middleware(request: NextRequest)` with `NextResponse.redirect()` and `NextResponse.rewrite()`.
  - Support `export const config = { matcher: ['/dashboard/:path*', '/((?!api|_next|favicon.ico).*)'] }`.
  - Allow inspecting cookies/headers and protecting private routes before SSR/page render.

---

## 6. 🚀 Backend Server Actions & Zero-Install Enhancements (Priority: P1)

- [ ] **Direct `"use server"` Inline Directive Support**:
  - Enable inline `"use server"` functions inside feature files, automatically translating them to RPC calls.
- [ ] **`core` Backend Library Pre-bundling**:
  - Bundle `z` (Zod), `dateFns`, `uuid`, `crypto`, `jwt`, `lodash` into `import { ... } from "core"` so backend use-cases never need `bun install` for common tasks.
- [ ] **Global Native Module Store (`~/.com/native-cache`)**:
  - Centralized cache for native C/C++ packages (e.g. `sharp`, `sqlite3`) across all local projects.

---

## 7. 📦 Production Build & Deployment Artifacts (Priority: P1)

- [ ] **`output: "standalone"` Server Bundle**:
  - Package the app into a single standalone Bun server runnable with `bun run server.js` (no dev dependencies needed).
- [ ] **`output: "export"` Static Site Generation (SSG)**:
  - Export all pages as pre-rendered static `.html` + `.js` files for Cloudflare Pages / Vercel / Nginx / S3 hosting.
- [ ] **Docker Deployment Generator**:
  - `com docker` command to generate an ultra-lightweight multi-stage Dockerfile based on `oven/bun:alpine`.

---

## 8. 📱 Zalo Mini App Engine Enhancements (Priority: P1)

- [ ] **Zalo SDK Zero-Config Bridge**:
  - Auto-polyfill `zmp-sdk` APIs in browser simulator with mocked user authentication, phone number request, and location.
- [ ] **ZMP Automated Packager**:
  - `com build --target zalo` produces clean ZMP-compliant zip artifacts ready to upload to Zalo Developer Portal.
