# `router` Module

The `router` module is a high-performance Next.js App Router matcher and scanner built entirely in Rust.

---

## 📁 File Structure

```
src/router/
├── mod.rs        # Public facade (AppRouter)
├── types.rs      # Core domain models (RouteEntry, RouteMatch, RouteSegment, SegmentType)
├── segment.rs    # Segment parser and classifier (Static, Dynamic, CatchAll, RouteGroup, ParallelSlot, Intercepting)
├── matcher.rs    # URL path normalizer (UTF-8 URL decoding, slashes, query params) & precedence matcher
├── scanner.rs    # Recursive filesystem scanner with layout cascade inheritance
└── tests.rs      # Comprehensive test suite (235 tests)
```

---

## 🎯 Supported Route Types & Conventions

| Route Pattern | Next.js Convention | Example Request Path | Captured Parameters |
| :--- | :--- | :--- | :--- |
| **Static Route** | `app/about/page.tsx` | `/about` | `{}` |
| **Dynamic Segment** | `app/users/[id]/page.tsx` | `/users/42` | `{"id": "42"}` |
| **Nested Dynamics** | `app/shop/[cat]/[prod]/page.tsx` | `/shop/laptops/macbook-pro` | `{"cat": "laptops", "prod": "macbook-pro"}` |
| **Catch-All** | `app/docs/[...slug]/page.tsx` | `/docs/api/v1/auth` | `{"slug": "api/v1/auth"}` |
| **Optional Catch-All** | `app/blog/[[...slug]]/page.tsx` | `/blog` OR `/blog/post-1` | `{}` OR `{"slug": "post-1"}` |
| **Route Group** | `app/(marketing)/about/page.tsx` | `/about` | `{}` *(Group is omitted from URL)* |
| **Parallel Slots** | `app/@modal/login/page.tsx` | Metadata tag | Used for parallel layout composition |
| **API Endpoints** | `app/api/users/route.ts` | `/api/users` | Marked as `is_api = true` |

---

## ⚖️ Precedence & Ranking Algorithm

When multiple route patterns could potentially match a given request path, the router applies a deterministic precedence score:

$$\text{Score} = \sum \text{Segment Score}$$

- **Static Segment**: `+100`
- **Dynamic Segment (`[id]`)**: `+50`
- **Optional Catch-All (`[[...slug]]`)**: `+20`
- **Catch-All (`[...slug]`)**: `+10`
- **Root Exact Match (`/`)**: `+1000`

**Example:**
For URL `/posts/new`:
1. `/posts/new` (Static: $100 + 100 = 200$) $\rightarrow$ **MATCHED**
2. `/posts/[id]` (Dynamic: $100 + 50 = 150$)
3. `/posts/[...slug]` (Catch-all: $100 + 10 = 110$)

---

## 🛡️ Normalization & Security Features

1. **Duplicate Slash Elimination**: `///users////123///` is normalized to `/users/123`.
2. **Query String & Hash Extraction**: `/search?q=rust#heading` parses query into `HashMap<String, String>` and strips the hash fragment.
3. **Full UTF-8 URL Percent Decoding**: `/s%E1%BA%A3n-ph%E1%BA%A9m` correctly decodes into `/sản-phẩm`.

---

## 🧪 Testing

Run all 235 router unit & integration tests:
```bash
cargo test router::tests
```
