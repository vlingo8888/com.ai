# So sánh com.ai.vn với Next.js App Router

> Đối chiếu với **Next.js 16** (App Router). Kiểm tra lần cuối: 2026-10-22, dựa trên code trong `compiler-rs/src` và bộ compat `tests/compat` (`bun run compat`).
> Đây là tài liệu "Khác biệt so với Next" ghi trong ROADMAP P4. Mỗi lần thêm hoặc bỏ tính năng thì cập nhật dòng tương ứng.

**Ký hiệu:** ✅ có, đã có test compat · ☑️ có code, chưa có test compat · 🟡 một phần / khác Next · ❌ chưa có · ⛔ không định hỗ trợ

**Kết quả compat ngày 2026-10-22:** 110/110 check pass trên 23 app (13 app trong `tests/compat/apps` và 10 mẫu từ `vercel/next.js/examples` trong `tests/compat/examples`), không còn `knownGap` nào. Lưu ý: bộ compat chỉ chạy `com dev`, chưa kiểm tra `com build` / `com start`. Vì vậy mọi dòng ở mục 8 đều chưa có test compat.

**Cách kiểm tra lại:**
- `bun run compat`: chạy các app Next mẫu trong `tests/compat/apps/*`. Check gắn `knownGap` là chỗ thiếu đã biết.
- Thêm một tính năng vào bảng: viết check trong `compat.json` của app phù hợp trước, rồi mới đổi trạng thái sang ✅.
- Option `next.config` không hỗ trợ sẽ được in cảnh báo khi chạy `com dev`.

---

## 1. Routing và file đặc biệt

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| `page`, `layout`, route lồng nhau | ✅ | |
| `[id]`, `[...slug]`, `[[...slug]]`, route group `(x)` | ✅ | khớp route bằng `matchit` |
| `loading`, `error`, `not-found`, `template` | ✅ | `loading` stream thật |
| `global-error` | ✅ | |
| `forbidden.tsx`, `unauthorized.tsx` + `forbidden()`, `unauthorized()` | ☑️ | |
| `global-not-found.tsx` (Next 15.4+) | ❌ | |
| Parallel routes `@slot` + `default.tsx` | ✅ | `router/parallel.rs` |
| Intercepting routes `(.)`, `(..)`, `(...)` | ✅ | |
| `route.ts` (GET/POST/…, `NextRequest`/`NextResponse`, streaming) | ✅ | |
| `middleware.ts` / `proxy.ts` (Next 16), `matcher` | ✅ | chạy trong worker Bun, không phải Edge runtime |
| `instrumentation.ts` + `onRequestError` | ☑️ | |
| `instrumentation-client.ts`, `onRouterTransitionStart` | ❌ | |
| i18n theo segment `[locale]`, `next-intl` | ✅ | |
| Pages Router (`pages/`, `getServerSideProps`, `getStaticProps`, `_app`, `_document`) | ⛔ | chỉ hỗ trợ App Router |

## 2. Rendering, RSC, Server Actions

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| React Server Components, page/layout `async` | ✅ | `react-server-dom-webpack`, 2 layer trong 1 process Bun |
| Streaming SSR + `<Suspense>` | ✅ | |
| `"use client"` / `"use server"` (file và inline) | ✅ | |
| Server Actions: form không JS, `useActionState`, `redirect()` trong action | ✅ | |
| Mã hoá biến closure của action inline | ❌ | Next mã hoá, com gửi nguyên → **rủi ro bảo mật** |
| `serverActions.allowedOrigins`, kiểm tra Origin chống CSRF | ❌ | **cần trước khi chạy production** |
| `serverActions.bodySizeLimit` | ❌ | |
| Gọi `modules/**` từ client | 🟡 | đi qua proxy `/_nata/rpc` riêng, không phải server reference của React |
| `useServerInsertedHTML` | 🟡 | là hàm rỗng: CSS-in-JS (styled-components, Emotion, MUI) **không chèn được style lúc SSR** |
| Partial Prerendering / `cacheComponents` (Next 16) | ❌ | ROADMAP P2, làm sau cùng |
| React Compiler (`reactCompiler`) | ❌ | option bị bỏ qua, không có cảnh báo |
| View Transitions (`experimental.viewTransition`) | ❌ | |
| Edge runtime (`export const runtime = "edge"`) | 🟡 | được chấp nhận nhưng mọi thứ đều chạy trên Bun |

## 3. Dữ liệu và cache

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| `fetch(..., { next: { revalidate, tags } })` | ✅ | |
| `"use cache"`, `cacheLife`, `cacheTag` | ✅ | cache store trong Rust (LRU + đĩa) |
| `"use cache: private"` / `"use cache: remote"` | 🟡 | nhận được directive, xử lý như `"use cache"` |
| `revalidatePath`, `revalidateTag`, `updateTag`, `refresh`, `unstable_cache`, `noStore` | ✅ | |
| `cacheLife` profile tuỳ chỉnh trong `next.config` | ❌ | chỉ có profile mặc định (`CACHE_PROFILES`) |
| Route segment config (`dynamic`, `revalidate`, `fetchCache`, `maxDuration`…) | ✅ | `runtime`, `preferredRegion` được chấp nhận rồi bỏ qua |
| `generateStaticParams`, `dynamicParams` | ✅ | |
| `after()`, `connection()` | ✅ | |
| `cookies()`, `headers()`, `draftMode()` (async) | ✅ | |
| ISR (trả bản cũ, revalidate chạy nền) | ☑️ | có trong `com start` |

## 4. Metadata và SEO

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| `metadata`, `generateMetadata`, title template | ✅ | |
| `robots.ts`, `sitemap.ts`, `manifest.ts` | ✅ | |
| `icon`, `apple-icon`, `opengraph-image`, `twitter-image` (file tĩnh và `.tsx`) | ☑️ | |
| `next/og` `ImageResponse` | ☑️ | Satori + resvg |
| `export const viewport` / `generateViewport` | ❌ | viewport đang viết cứng trong `rsc/mod.rs` |
| `generateSitemaps` (nhiều sitemap) | ❌ | |
| `generateImageMetadata` | ❌ | |

## 5. Các gói `next/*`

| Gói | Trạng thái | Ghi chú |
|---|---|---|
| `next/link` | ✅ | prefetch theo viewport, tự tắt khi mạng chậm |
| `next/navigation` (`useRouter`, `usePathname`, `useSearchParams`, `useParams`, `useSelectedLayoutSegment(s)`, `redirect`, `permanentRedirect`, `notFound`, `unstable_rethrow`) | ✅ | |
| `useLinkStatus`, `<Link onNavigate>` (Next 15.3+) | ❌ | |
| `next/image` (`/_next/image`, resize, WebP/AVIF, `remotePatterns`) | ✅ | Rust (`fast_image_resize`), không cần sharp |
| `next/legacy/image` | ❌ | |
| `next/font/google`, `next/font/local` | ✅ | tự host lúc build |
| `next/dynamic` | ✅ | |
| `next/script` | ✅ | strategy `worker` bị bỏ qua |
| `next/headers`, `next/server`, `next/cache`, `next/og` | ✅ | |
| `next/head`, `next/router` | 🟡 | bản tương thích tối thiểu của Pages Router |
| `next/form` | ❌ | |
| `next/web-vitals` (`useReportWebVitals`) | ❌ | |
| `@next/third-parties` | ❌ | |
| `@next/mdx` | ❌ | watcher có theo dõi `.mdx` nhưng chưa biên dịch |
| `@next/bundle-analyzer` | ❌ | |

## 6. Styling

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Tailwind v4 (compile ở server) | ✅ | compiler Rust trong engine, không cần cài `tailwindcss`; `@config`, `@plugin` khác typography/forms, `@import` package CSS thì dùng `@tailwindcss/node` trong worker. CSS khớp từng byte với bản JS trên 20/23 app compat; khác duy nhất `rounded-full` (`2147483647px` thay vì `3.40282e38px`, do lightningcss alpha.72) |
| Tailwind v3 (`@tailwind base`, `tailwind.config.*`) | 🟡 | compile với ngữ nghĩa v4, `tailwind.config` bị bỏ qua (trước đây dùng `@tailwindcss/browser`, cũng bỏ qua config) |
| `import "./x.css"`, CSS Modules | ✅ | Lightning CSS |
| Sass / SCSS | ❌ | watcher có theo dõi nhưng không biên dịch |
| `postcss.config` tuỳ chỉnh (plugin khác Tailwind) | ❌ | |
| CSS-in-JS khi SSR (styled-components, Emotion) | ❌ | xem `useServerInsertedHTML` ở mục 2 |

## 7. `next.config`

Các option `basePath`, `trailingSlash`, `redirects`, `rewrites`, `headers`, `env`, `images`, `serverExternalPackages` có tác dụng. Các option còn lại:

| Option | Trạng thái | Ghi chú |
|---|---|---|
| `transpilePackages`, `compiler` (`removeConsole`, `styledComponents`…), `experimental.*`, `reactStrictMode`, `poweredByHeader`, `eslint`, `typescript`, `devIndicators` | 🟡 | **bị bỏ qua mà không có cảnh báo** vì nằm trong danh sách `SUPPORTED` của `next_config.rs` |
| `output: "standalone"` | ❌ | ROADMAP P3.3 |
| `output: "export"` | ❌ | ROADMAP P3.3 |
| `assetPrefix`, `generateBuildId`, `skipTrailingSlashRedirect`, `cacheLife`, `typedRoutes`, `webpack`, `turbopack` | ❌ | có cảnh báo khi chạy |
| `i18n` (kiểu Pages Router) | ⛔ | |

## 8. Build, production, deploy

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| `com build`: prerender trang tĩnh, ISR, manifest, bảng route | ☑️ | `compiler-rs/src/build/` (ROADMAP vẫn ghi P3.1 là chưa làm, cần cập nhật) |
| Bundler production | 🟡 | đang dùng `Bun.build`; Rolldown chỉ có sau `COM_BUNDLER=rolldown` cho dependency |
| `com start`: trang từ RAM, nén brotli/gzip sẵn, ETag/304, health check, graceful shutdown | ☑️ | `prod.rs` |
| HTTP/2, `103 Early Hints` | ❌ | |
| Worker pool tự co giãn, tái chế worker khi RSS cao | ❌ | ROADMAP G3 |
| Source map ở production và trong overlay lỗi | ❌ | `sourcemap: "none"`, overlay hiển thị code đã biên dịch |
| Dockerfile mẫu, `com docker` | ❌ | |
| Zip Zalo Mini App (`com build --target zalo`) | 🟡 | đã prerender payload, chưa đóng gói zip |
| Deployment adapters (Vercel, Netlify…) | ⛔ | |

## 9. Trải nghiệm dev

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Fast Refresh giữ state | ✅ | |
| Overlay lỗi | 🟡 | chưa có source map, lỗi server chưa đẩy về overlay |
| Typecheck nền, `PageProps`/`LayoutProps` sinh tự động (`next typegen`) | ❌ | |
| `com lint`, `com doctor` | ☑️ | có lệnh, chưa đối chiếu với `next lint` / `next info` |
| `com dev --profile` (thời gian từng giai đoạn của request) | ❌ | ROADMAP G6 |

---

## 10. Nên làm gì để tốt hơn Next

Thứ tự theo ảnh hưởng: chặn đường lên production trước, sau đó mới đến các tính năng làm nên khác biệt.

### Bắt buộc trước khi dùng production

1. **Bảo mật Server Actions:** kiểm tra `Origin`/`Host` (`allowedOrigins`), mã hoá biến closure bằng AES-GCM với khoá sinh mỗi lần build (cách Next làm), thêm `bodySizeLimit`. Phần này làm trong Rust ở tầng nhận request `Next-Action`, chặn trước khi tới worker.
2. **Cảnh báo option bị bỏ qua:** xoá các key chưa có tác dụng khỏi `SUPPORTED` trong `next_config.rs`. Nếu không, người dùng tưởng `compiler.removeConsole` hay `transpilePackages` đang chạy.
3. **Source map end-to-end:** SWC sinh map, client bundle bật `sourcemap: "linked"`, overlay và log lỗi server map ngược về `.tsx` gốc. Hiện đây là chỗ DX kém Next rõ nhất.
4. **Kéo RAM dev về < 80MB:** hiện ~200MB, trong khi baseline là 74MB, nên cần tìm thay đổi gây tăng. Bật `bench.ts --budget` thành bắt buộc trong CI.

### Tương thích còn thiếu, theo mức độ hay gặp trong app thật

5. `viewport` / `generateViewport`: rất phổ biến (`themeColor` đã chuyển từ `metadata` sang đây từ Next 14).
6. `useServerInsertedHTML` thật: mở đường cho styled-components, Emotion, MUI, Chakra.
7. SCSS: biên dịch bằng crate `grass` (Sass viết bằng Rust) rồi đưa qua Lightning CSS, không cần `sass` của npm.
8. `output: "standalone"` và `"export"`, kèm Dockerfile `oven/bun:alpine`.
9. MDX: dùng `mdxjs-rs` (bản Rust chính thức của MDX), cắm vào pipeline SWC.
10. `next/form`, `useLinkStatus`, `useReportWebVitals`, `global-not-found`, `generateSitemaps`.

### Công nghệ để vượt Next (lợi thế có được nhờ engine Rust)

| Hướng | Công nghệ | Vì sao hơn Next |
|---|---|---|
| Bundler production | **Rolldown**, chunk theo route và theo client boundary | Chạy trong process Rust, không phải gọi sang Bun; tree-shaking tốt hơn `Bun.build` |
| Transform + minify | **Oxc** (transformer, minifier), đánh giá thay SWC theo P3.4 | Nhanh hơn SWC 2–3 lần, binary nhỏ hơn |
| Lint | **oxlint** trong `com lint` | Nhanh hơn ESLint 50–100 lần; Next 16 đã bỏ `next lint` |
| Typecheck nền | **`tsgo`** (TypeScript 7, viết bằng Go), chạy trong process riêng, báo lỗi vào overlay | Nhanh hơn `tsc` khoảng 10 lần |
| PPR / `cacheComponents` | Shell tĩnh phục vụ từ RAM trong Rust, phần động stream từ worker | Next chạy cả hai phần trên Node; com trả shell mà không đụng tới JS |
| Mạng | HTTP/2 mặc định, `103 Early Hints` cho CSS và modulepreload, HTTP/3 (`quinn` + `h3`) ở P5 | Next tự host không có Early Hints, cũng không có HTTP/3 |
| Production server | Worker pool tự co giãn, tái chế worker khi RSS > ngưỡng, pool Postgres dùng chung | Ít RAM hơn hẳn `next start` (Node), chống rò rỉ bộ nhớ từ code người dùng |
| Ảnh và font | Đã có bằng Rust; thêm subset font theo ký tự dùng (giữ đủ tiếng Việt) | Không cần `sharp`, file font nhỏ hơn |
| Zalo Mini App | Bundle offline + zip đúng chuẩn, ngân sách JS < 150KB gzip | Next không có |
| Đo và chặn hồi quy | So trực tiếp với `next dev`/`next start` trên cùng app trong CI mỗi PR | Biến "nhanh hơn Next" thành con số có kiểm chứng |

### Không nên làm

- **Pages Router:** khối lượng lớn mà app mới không dùng. Ghi rõ là không hỗ trợ.
- **Edge runtime thật:** Bun đã nhanh; chỉ cần chấp nhận `runtime = "edge"` để code Next chạy được.
- **`webpack()` / `turbopack` trong `next.config`:** không tương thích được về bản chất. Cảnh báo, và gợi ý cách thay thế.
- **Nhúng V8 (`deno_core`):** chỉ làm khi đo thấy IPC chiếm > 20% TTFB (ROADMAP P5).
