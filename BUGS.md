[x] - Không tự refesh sau khi code thay đổi (Zalo: runtime giờ dùng HMR WebSocket, tự fallback polling `/_nata/hmr_version` khi tunnel chặn WebSocket)
[] - Template `com create` khai báo `@pglite/core` nhưng `core.ts` (rpc.rs) `require("@electric-sql/pglite")`: project mới không có DB thật, rơi về mock driver (log "Falling back to mock driver").
