# ROADMAP: com.ai.vn thay thế Next.js App Router, native và tối ưu

> Mục tiêu: một app Next.js App Router (Next 15/16) chạy **không sửa code** bằng `com dev` / `com build` / `com start`, nhẹ và nhanh hơn Next rõ rệt. Làm tuần tự theo giai đoạn; mỗi giai đoạn có tiêu chí "xong" đo được.
>
> Quy ước: `[x]` xong · `[ ]` chưa làm · **(M)** bắt buộc để tương thích Next · **(O)** tối ưu hiệu năng/độ nhẹ.
> Mỗi khi bắt đầu một mục có công nghệ bên thứ ba, kiểm tra lại phiên bản ổn định mới nhất trước khi khoá lựa chọn.

---

## 1. Ngân sách hiệu năng (đo trên cùng một app, so với Next)

| Chỉ số | Mục tiêu | Hiện tại | Cách đo |
|---|---|---|---|
| `com dev` sẵn sàng | < 300ms | 70–380ms (tuỳ cache `bun install`) | dòng ` ✓ Ready in` |
| Trang đầu tiên sau khi khởi động (cold) | < 500ms | 520ms (request SSR đầu); 709ms đến khi hydrate xong, 223ms ở lần chạy thứ 2 nhờ cache | `scripts/bench.ts` |
| TTFB SSR khi đã ấm (dev) | < 10ms | 0.5ms median, 1.2ms p90 (trang không async) | `scripts/bench.ts` |
| TTFB SSR production | < 3ms (tĩnh), < 15ms (động) | — | `oha` |
| HMR: lưu file → thấy trên màn hình | < 50ms | ~100–300ms (re-render cả route) | timestamp watcher → `performance.now()` client |
| RAM khi rảnh (dev, engine + worker) | < 80MB | 74MB (đo sau khi GC dọn xong; ngay sau loạt request là ~80–105MB); 0MB sau 10 phút rảnh | `scripts/bench.ts` |
| RAM production (`com start`) | < 50MB | — | `ps -o rss` |
| JS client trang hello world | < 70KB gzip | 704KB ở dev (React **bản dev**, chưa nén; mục tiêu này dành cho production, P3) | `scripts/bench.ts` |
| Binary engine | < 20MB | 8.5MB | `ls -l` |

Việc đầu tiên của mỗi giai đoạn là chạy lại bảng này (`bun run bench`) và ghi số vào cột "Hiện tại". Số đo trên MacBook (Apple Silicon), app `tests/compat/apps/starter`, sau khi xong P0.

---

## 2. Lựa chọn công nghệ (chốt trước khi làm)

| Lớp | Chọn | Vì sao | Phương án thay thế đã cân nhắc |
|---|---|---|---|
| HTTP server | **Rust: axum + hyper** (giữ) | Nhanh, ít RAM, đã có; thêm HTTP/2, nén, ETag | actix-web (không đáng đổi) |
| Parse/transform TS/JSX | **SWC** (giữ ở giai đoạn đầu) → đánh giá **Oxc** ở P3 | SWC đã chạy ổn, có hệ sinh thái transform cho RSC/Server Actions/Fast Refresh. Oxc nhanh và nhỏ hơn nhưng phải tự viết các transform đó | Babel (quá chậm) |
| Resolve module | **oxc_resolver** (Rust) | Tương thích `enhanced-resolve` của webpack: `exports`/`imports` field, condition `react-server`, tsconfig paths. Thay resolver tự viết trong `compiler-rs/src/resolver/` | Tự viết tiếp (rủi ro sai edge case) |
| Bundler production | **Rolldown** (Rust, API kiểu Rollup) | Code splitting, tree-shaking, chạy trong process Rust, cùng hệ sinh thái Oxc | `Bun.build` (dễ nhưng phải gọi sang process Bun, kém kiểm soát chunk); esbuild (Go, phải gọi binary ngoài) |
| Minify JS | **Oxc minifier** hoặc minifier đi kèm Rolldown | Native, nhanh | terser (JS, chậm) |
| CSS | **Lightning CSS** (đã có trong `Cargo.toml`) + **Tailwind v4 compile ở server** (`@tailwindcss/node` + Oxide, chạy trong worker) | Bỏ hẳn `@tailwindcss/browser` khỏi client (~270KB JS + JIT tốn CPU trên máy yếu/Zalo). Lightning CSS lo minify, nesting, vendor prefix, CSS Modules | Tiếp tục JIT trên browser (nặng, FOUC) |
| Runtime chạy server code | **Bun** (worker thường trực, đã có) | Khởi động nhanh, ít RAM hơn Node, chạy TS trực tiếp, tương thích npm | Nhúng V8 bằng `deno_core` vào binary Rust (một process, không IPC, nhưng binary +30MB và phải tự lo tương thích npm/Node API). Đánh giá lại ở P5 |
| IPC Rust ↔ Bun | **Frame nhị phân có độ dài qua stdin/stdout của worker** (đã làm ở P0.2) | Nhiều request song song trên một kết nối, body là bytes thô; pipe chạy giống nhau trên macOS/Linux/Windows nên không cần nhánh named pipe | Unix socket (cùng hiệu năng, nhưng phải có thêm named pipe cho Windows); HTTP nội bộ (thêm overhead) |
| React | **React 19** + `react-server-dom-webpack` với shim `__webpack_require__` | Bản Flight được dùng nhiều nhất, Next cũng dùng; shim cho phép dùng mà không cần webpack | `react-server-dom-esm` (hợp ESM hơn nhưng ít người dùng; spike rồi quyết) |
| Cài gói | **`bun install`** + global cache của Bun (hardlink) | Bun installer thuộc loại nhanh nhất hiện nay; không tự viết package manager | Tự viết store kiểu pnpm (tốn công, ít lợi) |
| Gói cho browser (dev) | **Pre-bundle từng dependency bằng `Bun.build`** vào `.nata/deps/<pkg>@<ver>/`, phục vụ ở `/_nata/deps/*` (đã làm ở P0.5); chuyển sang Rolldown cùng lúc với bundler production (P3) | Hết phụ thuộc esm.sh lúc chạy, offline được, version khớp server; mỗi gói một file, các import trần khác trỏ về `/_nata/deps/` nên React chỉ có một instance | esm.sh (cũ: chậm, version lệch, cần mạng) |
| Cache biên dịch | **Cache trên đĩa theo hash nội dung** (`blake3`) trong `.nata/cache/`, lưu bằng `redb` | Khởi động lần 2 gần như tức thì, giống persistent cache của Turbopack | Chỉ cache trong RAM (mất khi restart) |
| Ảnh | **`fast_image_resize`** + crate `image`; encode WebP (`webp`) / AVIF (`ravif`) | Native, nhanh, không cần sharp/libvips | sharp (cần native addon Node) |
| Font | Tải Google Fonts lúc build, tự host; tính metric fallback bằng **`ttf-parser`** | Giống `next/font`: không gọi Google lúc runtime, chống layout shift | Link thẳng Google Fonts (chậm, lộ request) |
| Nén response | **tower-http** compression: brotli cho asset tĩnh (nén sẵn lúc build), gzip/zstd cho động | Giảm băng thông; tĩnh nén một lần | Nén runtime cho mọi thứ (tốn CPU) |
| Watcher | **notify** (giữ) | Đã ổn | watchman (phụ thuộc ngoài) |
| DB dev | **PGlite**, nạp lười (chỉ khởi tạo khi có query đầu tiên) | Giảm RAM khi project không dùng DB | Postgres thật qua Docker (nặng) |
| Đo hiệu năng | **oha** (load test, Rust), **hyperfine**, `/usr/bin/time -l` | Đơn giản, chạy được trong CI | k6, autocannon |

---

## 3. Các giai đoạn

### P0: Nền móng nhẹ (làm trước mọi thứ)

- [x] **P0.1 Worker Bun thường trực** (O): `compiler-rs/src/worker_pool/`. SSR 36→4.6ms, API 44→4.1ms, RPC 43→4.2ms. `COM_WORKER=0` để quay về cách cũ.
- [x] **P0.2 IPC frame nhị phân, chạy song song** (O)
  - Frame `u32 length | u8 kind | u32 id | u32 meta length | meta JSON | body` qua stdin/stdout của worker (không dùng Unix socket: xem mục 2). Kind `request`, `result`, `ready`, `shutdown`; `chunk` để dành cho streaming ở P1.
  - Body request/response route handler là bytes thô (bỏ base64-trong-JSON). Lệnh có cấu trúc `ssr` / `route` / `rpc` gọi hàm đã nạp sẵn, không còn ghi file script tạm mỗi request; `eval` chỉ dùng cho test và endpoint metadata.
  - Kết quả: SSR 4.6→**0.5ms**, API 4.1→**0.2ms**, RPC 4.2→**0.9ms** (median). Test `concurrent_requests_run_in_parallel_with_isolated_state`, `request_bodies_round_trip_as_raw_bytes`.
- [x] **P0.3 Request context bằng `AsyncLocalStorage`** (M)
  - `__NATA_COOKIES__`, `__NATA_SET_COOKIES__`, `__NATA_QUERY_LOGS__`, `__NATA_PARAMS__`, console output và `window`/`document`/`localStorage` của SSR là accessor đọc từ store của request. `window` chỉ tồn tại khi render SSR, nên Server Action/route handler không bị thư viện nhận nhầm là browser.
  - PGlite: một kết nối được cấp như pool kích thước 1 (transaction giữ kết nối), re-entrant theo request (`__NATA_REQUEST__`) để `db` dùng trong transaction của chính request không tự chờ mình.
- [x] **P0.4 Script benchmark + compat harness** (M)
  - `bun run bench` (`scripts/bench.ts`): đo bảng ở mục 1, ngưỡng trong `scripts/perf-budget.json` (`--budget`), so baseline (`--compare`, `scripts/bench-baseline.json`).
  - `bun run compat` (`tests/compat/run.ts` + `tests/compat/lib/harness.ts`): 5 app Next 15 (`starter`, `async-data`, `server-actions`, `routing`, `ui-libraries` với Radix/sonner/lucide), driver CDP. Kết quả hiện tại: **21 pass, 3 known gap** (đều thuộc P1). Check có `knownGap` báo XFAIL; khi tự nhiên pass thì báo XPASS để gỡ.
  - CI: `.github/workflows/ci.yml` (cargo test, bun test, compat, budget).
  - Sau đó thêm `ui-libraries` từ lỗi người dùng gặp: import nhiều dòng trong bundle dependency không được viết lại (`Failed to resolve module specifier "@radix-ui/react-focus-scope"`); `require()` gói external bị Bun đổi thành binding hằng (`Assignment to constant variable` trong React dev jsx-runtime); `window`/`document` giả ở SSR khiến thư viện gọi DOM không tồn tại (`document.createTextNode is not a function`). Cả ba đã sửa, SSR giờ không có `window`/`document` giống Next.
  - Bộ compat đã bắt được 3 bug thật và đã sửa: `await params` (Next 15) làm treo trang do thenable tự resolve chính nó; catch-all `[...slug]` trả chuỗi thay vì mảng (giờ dùng `RouteEntry::params_json`, có decode `%xx`); shell chèn `body { background: var(--background) }` đè CSS của project.
- [x] **P0.5 Dependency pre-bundling, bỏ esm.sh** (M+O)
  - `com dev` chạy `bun install` khi `package.json` có gói chưa cài; gói được import mà chưa khai báo thì tự `bun add`.
  - `/_nata/deps/<specifier>` (`compiler-rs/src/deps.rs`): bundle bằng `Bun.build` trong process chạy một lần (`deps_bundler.ts`, để bộ nhớ của bundler không ở lại worker), CJS→ESM có named export, cache theo version, ETag/304. esm.sh chỉ còn là fallback khi không cài được gói.
  - Importmap chỉ còn React + shim `next/*`, đều local. React 19 cho cả client và server.
- [x] **P0.6 Tailwind v4 + CSS ở server** (O)
  - `compiler-rs/src/css_compiler/stylesheet.rs` + lệnh `css` trong worker: `@tailwindcss/node` + Oxide (tự cài thành devDependency nếu thiếu), inline vào `<style id="_nata_custom_css">`, bỏ `@tailwindcss/browser` ở web và Zalo (fallback về browser chỉ khi không cài được Tailwind).
  - CSS có `@import "tailwindcss"` được compile đúng như viết (giống Next); project cũ không import thì được chèn theme cũ để giữ giao diện.
  - `import "./x.css"` và CSS Modules (`/_nata/css/*`, Lightning CSS), tên class `<local>_<fnv1a(path)>` giống nhau ở SSR (Bun plugin trong worker) và client.
  - HMR: sửa code thì quét lại class, sửa CSS thì compile lại, chỉ thay stylesheet.
- [x] **P0.7 Cache biên dịch trên đĩa** (O)
  - `compiler-rs/src/compile_cache.rs`: key `blake3(bản build engine, đường dẫn, nội dung, tsconfig)`, `redb` trong `.nata/cache/`, tự xoá khi quá `COM_CACHE_MAX_MB` (256). 6 module: 103ms → 26ms; trang đầu tiên ở lần chạy thứ 2: 709ms → 223ms.
- [x] **P0.8 Giảm RAM worker** (O)
  - PGlite và React nạp lười; `bun --smol`; worker tự tắt sau `COM_WORKER_IDLE` giây rảnh (mặc định 600) và bật lại khi có request (~60ms); bundle dependency chạy ngoài worker.
  - Engine: allocator `mimalloc`, tokio 2 luồng ở dev (`COM_THREADS`), regex của route được biên dịch một lần thay vì mỗi request.
  - RAM rảnh: 70MB (không Tailwind) – 91MB (có Tailwind compile trong worker).

**Còn lại sau P0 (chuyển sang P1/P3):**
- Page/layout `async` vẫn render ở client (`renderToString` không chờ được async component; `renderToReadableStream` đã thử và chạy được, nhưng phải đi cùng RSC ở P1.5 để không bị hydration mismatch). Tạm thời đã làm giống Next ở phần hiển thị: HTML chứa sẵn `loading.tsx` gần nhất (render ở server, bỏ màn "Mounting App Router"), và ở client component `async` suspend thật nên `loading.tsx` hiện cả khi chuyển trang.
- File `"use server"` chưa được tách: action bị bundle xuống client và chạy ở browser (P1.7). Đây cũng là rủi ro lộ code server.
- JS client dev 704KB (React bản dev, chưa nén): nén response (G5) và chunk production ở P3.
- Template `com create` khai báo `@pglite/core` trong khi `core.ts` dùng `@electric-sql/pglite`, nên project mới rơi về mock DB (xem BUGS.md).

**Xong P0 khi:** 4 app compat chạy được `com dev` (chấp nhận còn đang render phía client); bảng ngân sách cập nhật; offline được.

---

### P1: Module graph thống nhất + React Server Components (lõi tương thích Next)

- [ ] **P1.1 Chuyển resolver sang `oxc_resolver`** (M)
  - Hỗ trợ condition `react-server` / `browser` / `import` / `default`, field `exports`, tsconfig `paths`.
  - Giữ test hiện có trong `compiler-rs/src/resolver/tests.rs` và bổ sung case `exports` lồng nhau.
- [ ] **P1.2 Module graph trong Rust** (M+O)
  - Mỗi module: id, file, hash, danh sách import, layer (`server` | `client` | `ssr`), directive.
  - Pass SWC phát hiện `"use client"` / `"use server"` ở đầu file và `"use server"` trong thân hàm.
  - Graph là nguồn sự thật cho: invalidate HMR, client manifest, server action manifest, code splitting production.
- [ ] **P1.3 Rust phục vụ module cho Bun** (O)
  - Bun nạp code server qua `Bun.plugin` (`onResolve`/`onLoad`) hỏi Rust qua IPC, nên client và server dùng **một trình biên dịch duy nhất** (cùng transform, cùng directive).
  - Khi file đổi, Rust gắn version mới vào specifier của file đó và các importer của nó (`?v=hash`), worker không cần restart. Bỏ cơ chế restart toàn bộ worker của P0.1.
  - Xong khi: sửa một use-case, request kế tiếp dùng code mới trong < 20ms, state khác của worker vẫn giữ.
- [ ] **P1.4 Spike chọn bản Flight** (M): `react-server-dom-webpack` + shim, hay `react-server-dom-esm`. Tiêu chí: chạy trên Bun, stream được, bundle client nhỏ. Ghi lại quyết định vào mục 2.
- [ ] **P1.5 Render pipeline RSC** (M)
  - Worker có 2 tầng:
    - **RSC**: Bun chạy với condition `react-server`, render cây `layout`/`template`/`page` ra Flight. Server component chạy thật trên server, gọi `db` trực tiếp.
    - **SSR**: `react-dom/server` `renderToReadableStream` đọc Flight, stream HTML về Rust, Rust stream tiếp ra client. Flight chunk được nhúng vào HTML qua `<script>`.
  - `loading.tsx` thành `<Suspense>` với streaming thật; `error.tsx` là error boundary client; `not-found.tsx`, `notFound()`, `redirect()` xử lý trên server (status 404/307 đúng).
  - Bỏ `makeComponent` và `unwrapHtmlBody` trong `compiler-rs/src/client/runtime.js` (các hack chạy async component trên client).
- [ ] **P1.6 Client router kiểu Next** (M)
  - Hydrate bằng `createFromReadableStream`; chuyển trang bằng `fetch(path, { headers: { RSC: "1" } })` + `createFromFetch`.
  - Router cache theo segment, giữ state của layout dùng chung khi chuyển trang.
  - `Link` prefetch khi lọt vào viewport (IntersectionObserver); `router.refresh()`, `router.prefetch()`.
  - Scroll restoration; `useSelectedLayoutSegment(s)`.
- [ ] **P1.7 Server Actions chuẩn Next** (M)
  - `"use server"` dạng file và inline; action id = hash(file + export).
  - `encodeReply` / `decodeReply`; `<form action={fn}>` chạy được cả khi chưa có JS (progressive enhancement).
  - `useActionState`, `useFormStatus`, `useOptimistic`.
  - `revalidatePath` / `revalidateTag` / `refresh()` trả Flight mới trong cùng response.
  - RPC proxy `modules/` hiện có trở thành alias của action, để app cũ không vỡ.
- [ ] **P1.8 React Fast Refresh** (O)
  - SWC `react_refresh` transform + `react-refresh/runtime` cho component client: giữ state khi sửa.
  - Sửa server component thì chỉ refetch Flight của route hiện tại.
  - Xong khi: lưu file → màn hình cập nhật < 50ms, state input không mất.
- [ ] **P1.9 Zalo Mini App** (M cho sản phẩm)
  - Vẫn chạy client-only trên thiết bị. Server component động lấy Flight từ backend (tunnel ở dev, URL production khi build).
  - Trang tĩnh được prerender lúc `com build --target zalo`.

**Xong P1 khi:** app compat số 2 và số 3 chạy `com dev` với server component thật (page `async` không còn gọi RPC); JS client trang hello world < 100KB gzip.

---

### P2: Đầy đủ API Next.js

**Routing**
- [ ] `middleware.ts` (M): `matcher` compile thành regex trong Rust; chạy trong worker trước khi render; `NextResponse.next` / `redirect` / `rewrite`, sửa header/cookie. (Next 16 đổi tên thành `proxy.ts`, hỗ trợ cả hai.)
- [ ] Parallel routes `@slot` + `default.tsx` (M)
- [ ] Intercepting routes `(.)` `(..)` `(...)` (M)
- [ ] `route.ts` đầy đủ: `NextRequest` / `NextResponse`, `export const dynamic`, streaming response (M)
- [ ] `forbidden.tsx`, `unauthorized.tsx`, `global-error.tsx` (M)
- [ ] i18n theo segment `[locale]` + `next-intl` server (M)

**Dữ liệu & cache**
- [ ] `fetch` cache + `next: { revalidate, tags }` (M)
- [ ] `"use cache"`, `cacheLife`, `cacheTag` (M): cache store trong Rust (LRU trong RAM + đĩa)
- [ ] `unstable_cache`, `revalidatePath`, `revalidateTag`, `refresh` (M)
- [ ] Route segment config: `dynamic`, `revalidate`, `fetchCache`, `runtime`, `preferredRegion` (chấp nhận nhưng có thể bỏ qua), `maxDuration` (M)
- [ ] `generateStaticParams`, `dynamicParams` (M)
- [ ] `after()` (M)
- [ ] Partial Prerendering (O, làm sau cùng): shell tĩnh + lỗ động streaming

**Request APIs**
- [ ] `cookies()`, `headers()`, `draftMode()` async theo Next 15 (M), dựa trên ALS của P0.3
- [ ] `connection()`, `userAgent()` (M)

**Shims gói `next/*`**
- [ ] `next/font/google`, `next/font/local` (M+O): tải và tự host lúc build, sinh `@font-face` + fallback metric (`ttf-parser`), trả về `className` / `variable` / `style` giống Next
- [ ] `next/image` (M+O): endpoint `/_next/image?url=&w=&q=` (giữ đúng URL của Next), resize bằng `fast_image_resize`, WebP/AVIF, cache trên đĩa, `srcset` và `sizes` đúng
- [ ] `next/dynamic` (M): `ssr: false`, `loading`
- [ ] `next/script` (M): `beforeInteractive`, `afterInteractive`, `lazyOnload`, `worker` (bỏ qua)
- [ ] `next/og` `ImageResponse` (O): Satori + resvg (resvg là crate Rust)
- [ ] `next/server`: `NextRequest`, `NextResponse`, `userAgent`, `after` (M)
- [ ] `next.config.(js|ts|mjs)` (M): `basePath`, `redirects`, `rewrites`, `headers`, `images`, `env`, `trailingSlash`, `output`, `serverExternalPackages`, `experimental` (đọc và cảnh báo option chưa hỗ trợ)
- [ ] `instrumentation.ts` + `onRequestError` (M)
- [ ] Env: `.env*` theo thứ tự ưu tiên của Next; `NEXT_PUBLIC_*` inline vào bundle client (M)

**Xong P2 khi:** cả 4 app compat chạy `com dev` không sửa code; thêm 5–10 app thật (ví dụ các mẫu chính thức trong `vercel/next.js/examples`) vào bộ compat.

---

### P3: Production build & server

- [ ] **P3.1 `com build`** (M+O)
  - Module graph → Rolldown: chunk theo route và theo boundary client, tree-shaking, minify bằng Oxc.
  - Tên file theo hash nội dung, `Cache-Control: immutable`.
  - Prerender route tĩnh ra HTML + Flight; nén sẵn brotli/gzip.
  - Xuất `.nata/build/` với manifest (route, chunk, action, prerender); `com build` in bảng kích thước route giống Next (`○` static, `ƒ` dynamic, `◐` PPR).
- [ ] **P3.2 `com start`** (M+O)
  - Binary Rust phục vụ file tĩnh và trang prerender từ RAM; trang động chuyển sang worker pool (N = số CPU).
  - ISR: phục vụ bản cũ trong khi revalidate nền.
  - HTTP/2, ETag, `304`, nén.
  - Graceful shutdown; health check `/_nata/health`.
- [ ] **P3.3 Đóng gói** (M)
  - `output: "standalone"`: một thư mục gồm binary engine + `server/` đã bundle (không cần `node_modules`).
  - `output: "export"`: HTML/JS tĩnh cho Nginx/S3/Cloudflare Pages.
  - Dockerfile mẫu dựa trên `oven/bun:alpine` + binary engine (mục tiêu image < 120MB).
  - `com build --target zalo` → zip đúng chuẩn Zalo Mini App.
- [ ] **P3.4 Đánh giá Oxc thay SWC** (O): so sánh tốc độ transform, kích thước binary, mức độ đủ của transform (TS, JSX, Fast Refresh, directive). Chỉ chuyển nếu nhanh hơn ≥ 2 lần và đủ tính năng.

**Xong P3 khi:** app compat build + start được, Lighthouse ≥ Next cho cùng app, TTFB và RAM đạt ngân sách production.

---

### P4: Developer experience ngang Next

- [ ] Overlay lỗi có source map thật: SWC sinh source map, overlay map stack về file `.tsx` gốc (hiện đang hiển thị "compiled output")
- [ ] Lỗi server (RSC, action, middleware) được đẩy về overlay qua HMR socket, kèm code frame
- [ ] `com lint` (gọi `oxlint`, Rust) và typecheck nền (`tsc --noEmit` trong worker riêng, báo vào overlay)
- [ ] `com info`, `com doctor`: kiểm tra Bun version, port, env, config không hỗ trợ
- [ ] Codemod `com migrate-from-next`: thường không cần, chỉ xử lý `next.config` lạ và gói không hỗ trợ
- [ ] Tài liệu "Khác biệt so với Next": danh sách tính năng chưa hỗ trợ, cập nhật mỗi phiên bản

---

### P5: Tối ưu sâu (sau khi đã tương thích)

- [ ] Hồ sơ hoá (profiling) bằng `samply` / `cargo flamegraph` cho engine và `bun --inspect` cho worker; tối ưu điểm nóng
- [ ] Đánh giá nhúng JS engine thẳng vào binary (`deno_core`/V8) cho production: bỏ IPC, một process. Chỉ làm nếu đo thấy IPC chiếm > 20% TTFB
- [ ] Cache Flight theo segment ở edge của `com start` (tương đương Full Route Cache của Next)
- [ ] Streaming SSR không chặn: flush `<head>` và CSS trước khi dữ liệu xong (early hints `103`)
- [ ] Giảm JS client: tách router runtime, chỉ tải phần cần (prefetch, overlay chỉ ở dev)
- [ ] Bộ benchmark công khai so với Next (cùng app, cùng máy), chạy trong CI mỗi PR để chống chậm đi

---

## 4. Giải pháp giữ nhẹ lâu dài (G)

Các giai đoạn P0–P5 là để *trở nên* nhẹ. Phần này là các kỹ thuật và rào chắn để *luôn* nhẹ khi framework lớn dần. Mỗi mục gắn vào giai đoạn nên làm cùng.

### G1. Nguyên tắc kiến trúc (áp dụng cho mọi tính năng mới)
- **Không có JS mặc định:** server component không gửi code xuống client; chỉ `"use client"` mới tạo JS. Tính năng mới nào cũng phải trả lời "nó thêm bao nhiêu byte vào client?".
- **Lười mọi thứ:** không khởi tạo subsystem nào (SWC, CSS, PGlite, React trong worker, image encoder) cho đến lần dùng đầu tiên. Lúc khởi động chỉ bind port và quét route.
- **Stream thay vì buffer:** không giữ cả response trong RAM ở bất kỳ tầng nào (Rust, IPC, worker).
- **Tĩnh trước, động sau:** route nào prerender được thì prerender; phần động chỉ là "lỗ" trong trang tĩnh.
- **Dev chỉ làm việc theo yêu cầu:** chỉ compile route đang mở, không build trước toàn bộ project.

### G2. Engine Rust
- [x] **Allocator `mimalloc`** (crate `mimalloc`, `#[global_allocator]`) (P0): giảm phân mảnh và RSS khi chạy lâu, nhanh hơn allocator hệ thống khi nhiều luồng.
- [x] **Giới hạn luồng tokio ở dev** (P0): `worker_threads = 2` khi `com dev` (mỗi luồng tốn stack + buffer); production dùng số nhân CPU.
- [ ] **Khớp route bằng `matchit`** (radix tree, thứ axum dùng) thay regex cho từng route (P1): O(độ dài path), không phụ thuộc số route.
- [x] **Bỏ base64-trong-JSON khi gửi body sang worker** (P0.2): body gửi dạng frame nhị phân, metadata dạng MessagePack (`rmp-serde`). Hết phình 33% và hết copy.
- [ ] **Zero-copy:** dùng `bytes::Bytes`, `Arc<str>` cho nội dung module/CSS dùng chung; body response là stream (`axum::body::Body::from_stream`) (P1.5).
- [ ] **Mọi cache đều có giới hạn:** cache module, route info, ảnh, Flight dùng LRU có giới hạn theo dung lượng (`moka` hoặc `quick_cache`); không dùng `HashMap` tăng mãi (P0.7).
- [ ] **Giữ binary gọn:** `default-features = false` cho mọi crate, chỉ bật feature cần; kiểm tra `cargo bloat --release` và `cargo udeps` trong CI (G6).
- [ ] **Chỉ quét lại phần route bị đổi:** watcher cập nhật trie theo file thay đổi thay vì `AppRouter::scan` lại toàn bộ (P1.2).

### G3. Worker Bun
- [x] **Tắt khi rảnh ở dev** (P0.8): không có request trong 10 phút thì tắt worker (RAM về 0), bật lại khi có request đầu tiên (~60ms). Có thể cấu hình `COM_WORKER_IDLE`.
- [ ] **Pool tự co giãn ở production** (P3.2): tối thiểu 1, tối đa số CPU, tăng khi hàng đợi dài, giảm khi rảnh.
- [x] **`bun --smol`** ở dev (P0.8): GC tích cực hơn, heap nhỏ hơn.
- [x] **Một instance React và một PGlite cho mỗi worker**, nạp lười (P0.8).
- [x] **Không còn file tạm cho mỗi request** (P0.2): hiện worker ghi runner script ra `.nata/worker-scripts/` rồi import. Chuyển sang gửi lệnh có cấu trúc (`{kind: "ssr", input}`) cho các hàm đã nạp sẵn.
- [ ] **Giới hạn bộ nhớ và tự tái chế:** worker vượt ngưỡng RSS (vd 300MB) thì được thay bằng worker mới sau khi xử lý xong request đang chạy, chống rò rỉ bộ nhớ từ code người dùng (P3.2).

### G4. Client (trình duyệt và Zalo WebView)
- [ ] **Runtime router < 10KB gzip** (P1.6); overlay lỗi và debug drawer chỉ có ở dev và chỉ tải khi cần (`import()` lúc có lỗi đầu tiên hoặc khi mở drawer).
- [ ] **Target JS hiện đại:** ES2020+, không polyfill mặc định; target CSS lấy từ `browserslist` qua Lightning CSS. Có profile riêng cho Zalo WebView (Android cũ) nếu đo thấy cần (P3.1).
- [ ] **CSS quan trọng inline:** Lightning CSS tách CSS dùng cho route đầu tiên, inline vào `<head>`, phần còn lại tải bất đồng bộ (P3.1).
- [ ] **Preload đúng chỗ:** `<link rel="modulepreload">` cho chunk của route, `103 Early Hints` ở production (P3.2).
- [ ] **Prefetch có ý thức mạng:** prefetch `Link` khi vào viewport và trình duyệt rảnh (`requestIdleCallback`); tắt khi `navigator.connection.saveData` bật hoặc mạng 2G (P1.6).
- [ ] **Ảnh và font:** `loading="lazy"`, AVIF/WebP, `sizes` đúng; subset font theo ký tự dùng (giữ đủ tiếng Việt), `font-display: swap` (P2).
- [ ] **Hydrate chọn lọc:** khuyến khích boundary `<Suspense>` quanh client component nặng để React hydrate theo ưu tiên (P1.5).
- [ ] **Zalo:** asset nằm trong gói zip (không tải qua mạng lúc mở app); Eruda chỉ có ở dev; ngân sách JS riêng cho Zalo < 150KB gzip (P1.9, P3.3).

### G5. Dữ liệu và mạng
- [ ] **Gộp truy vấn trùng trong một request:** `React.cache` cho hàm đọc dữ liệu; dev cảnh báo N+1 khi cùng một câu SQL chạy hơn 5 lần trong một request (dựa trên query log sẵn có) (P1.5).
- [ ] **Pool kết nối Postgres nhỏ và dùng chung** trong worker (vd 5 kết nối), prepared statement (P3.2).
- [ ] **Nén đúng cách:** brotli nén sẵn lúc build cho asset tĩnh; brotli/gzip mức thấp cho response động (P3.2).
- [ ] **Cache HTTP chuẩn:** asset có hash → `immutable`; HTML tĩnh → `ETag` + `stale-while-revalidate` (P3.2).
- [ ] **HTTP/2 mặc định;** HTTP/3 (`quinn` + `h3`) chỉ đánh giá ở P5.

### G6. Rào chắn trong CI: không để framework nặng dần lên
- [x] **Ngân sách có hiệu lực:** script P0.4 chạy trên mỗi PR, **fail** nếu vượt ngân sách ở mục 1 hoặc chậm/nặng hơn bản trước > 10%. Đo: TTFB, RAM rảnh, thời gian khởi động, JS client theo từng route (gzip), kích thước binary.
- [ ] **Báo cáo kích thước theo route** sau mỗi `com build` (giống Next), lưu lịch sử để thấy route nào phình.
- [ ] **Chính sách dependency:** thêm crate hoặc gói npm vào engine/runtime phải kèm số đo trước/sau (kích thước binary, RAM, thời gian khởi động). Ưu tiên tự viết phần nhỏ thay vì kéo cả thư viện lớn.
- [ ] **Profiling định kỳ:** mỗi milestone chạy `samply` (engine) và `bun --inspect` (worker) trên app compat, ghi điểm nóng vào issue.
- [ ] **`com dev --profile`:** in thời gian từng giai đoạn của một request (resolve, compile, IPC, render, stream) để người dùng tự thấy chỗ chậm.

---

## 5. Thứ tự làm đề xuất (tuần tự, mỗi bước dùng được ngay)

1. P0.4 benchmark + compat harness, cùng G6 (rào chắn CI), để có số đo trước khi tối ưu
2. P0.5 pre-bundling + React 19, rồi P0.6 Tailwind ở server: giảm JS client mạnh nhất
3. G2 `mimalloc` + giới hạn luồng tokio, G3 tắt worker khi rảnh: thắng nhanh về RAM, mỗi việc chỉ vài giờ
4. P0.2 IPC socket (kèm bỏ base64 và file tạm theo G2/G3), rồi P0.3 ALS: nền cho streaming và chạy song song
5. P1.1 → P1.3 module graph thống nhất (kèm `matchit`)
6. P1.4 → P1.7 RSC + router + Server Actions (khối lớn nhất)
7. P1.8 Fast Refresh, P0.7 / P0.8 cache + RAM
8. P2 theo thứ tự: middleware → cache/revalidate → `next/font` + `next/image` → parallel/intercepting
9. P3 production (gồm các mục G4/G5 gắn nhãn P3), rồi P4, P5

## 6. Rủi ro chính và cách giảm

| Rủi ro | Giảm thiểu |
|---|---|
| API Flight của React thay đổi giữa các bản | Khoá version React chính xác; bọc mọi lời gọi Flight trong một module adapter duy nhất |
| Next ra tính năng mới nhanh hơn tốc độ đuổi theo | Bộ compat dựa trên app thật, ưu tiên tính năng theo tần suất xuất hiện trong app thật, không theo changelog |
| Rolldown / Oxc thay đổi API (crate còn trẻ) | Khoá version trong `Cargo.lock`, bọc sau trait nội bộ để có thể quay về `Bun.build` |
| Windows (Unix socket, đường dẫn) | Chạy CI trên Windows từ P0; named pipe làm fallback |
| PGlite đơn process không chạy được nhiều worker | Dev: 1 worker; production: dùng Postgres thật (`DATABASE_URL`), không chạy PGlite với worker pool |
