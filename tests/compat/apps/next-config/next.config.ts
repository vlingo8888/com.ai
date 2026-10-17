const config = {
  env: { CUSTOM_VALUE: "from-config" },
  async redirects() {
    return [{ source: "/old-blog/:slug", destination: "/real?from=:slug", permanent: true }];
  },
  async rewrites() {
    return {
      beforeFiles: [{ source: "/alias", destination: "/real" }],
      afterFiles: [{ source: "/legacy/:path*", destination: "/real" }],
      fallback: [],
    };
  },
  async headers() {
    return [{ source: "/(.*)", headers: [{ key: "x-com-config", value: "applied" }] }];
  },
};

export default config;
