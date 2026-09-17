use std::path::PathBuf;

use super::{
    matcher::RouteMatcher,
    segment::SegmentParser,
    types::{RouteEntry, RouteKind, SegmentType},
    AppRouter,
};

fn create_test_route(segments: &[&str], is_api: bool) -> RouteEntry {
    let parsed_segments = SegmentParser::parse_segments(segments);
    let (pattern, regex_str, param_names, score) =
        RouteMatcher::compile_segments(&parsed_segments);

    let file_suffix = if is_api { "route.ts" } else { "page.tsx" };
    let page_file = PathBuf::from(format!("app/{}/{}", segments.join("/"), file_suffix));

    RouteEntry {
        pattern,
        regex: regex_str,
        page_file,
        layout_files: vec![PathBuf::from("app/layout.tsx")],
        param_names,
        is_api,
        kind: if is_api { RouteKind::Api } else { RouteKind::Page },
        score,
        segments: parsed_segments,
    }
}

// =========================================================================
// SUITE 1: STATIC ROUTE MATCHING (50 TESTS)
// =========================================================================

macro_rules! test_static_route {
    ($test_name:ident, $segments:expr, $request_path:expr) => {
        #[test]
        fn $test_name() {
            let mut router = AppRouter::new();
            router.add_route(create_test_route($segments, false));
            let matched = router.match_route($request_path);
            assert!(matched.is_some(), "Expected route to match {}", $request_path);
            let (route, _) = matched.unwrap();
            assert!(!route.is_api);
        }
    };
}

test_static_route!(test_static_001_root, &[], "/");
test_static_route!(test_static_002_about, &["about"], "/about");
test_static_route!(test_static_003_contact, &["contact"], "/contact");
test_static_route!(test_static_004_pricing, &["pricing"], "/pricing");
test_static_route!(test_static_005_features, &["features"], "/features");
test_static_route!(test_static_006_terms, &["terms"], "/terms");
test_static_route!(test_static_007_privacy, &["privacy"], "/privacy");
test_static_route!(test_static_008_login, &["login"], "/login");
test_static_route!(test_static_009_register, &["register"], "/register");
test_static_route!(test_static_010_logout, &["logout"], "/logout");
test_static_route!(test_static_011_dashboard, &["dashboard"], "/dashboard");
test_static_route!(test_static_012_settings, &["settings"], "/settings");
test_static_route!(test_static_013_profile, &["profile"], "/profile");
test_static_route!(test_static_014_billing, &["billing"], "/billing");
test_static_route!(test_static_015_analytics, &["analytics"], "/analytics");
test_static_route!(test_static_016_docs, &["docs"], "/docs");
test_static_route!(test_static_017_blog, &["blog"], "/blog");
test_static_route!(test_static_018_changelog, &["changelog"], "/changelog");
test_static_route!(test_static_019_faq, &["faq"], "/faq");
test_static_route!(test_static_020_help, &["help"], "/help");
test_static_route!(test_static_021_status, &["status"], "/status");
test_static_route!(test_static_022_feed, &["feed"], "/feed");
test_static_route!(test_static_023_explore, &["explore"], "/explore");
test_static_route!(test_static_024_notifications, &["notifications"], "/notifications");
test_static_route!(test_static_025_messages, &["messages"], "/messages");
test_static_route!(test_static_026_search, &["search"], "/search");
test_static_route!(test_static_027_roadmap, &["roadmap"], "/roadmap");
test_static_route!(test_static_028_integrations, &["integrations"], "/integrations");
test_static_route!(test_static_029_careers, &["careers"], "/careers");
test_static_route!(test_static_030_press, &["press"], "/press");
test_static_route!(test_static_031_partners, &["partners"], "/partners");
test_static_route!(test_static_032_security, &["security"], "/security");
test_static_route!(test_static_033_community, &["community"], "/community");
test_static_route!(test_static_034_events, &["events"], "/events");
test_static_route!(test_static_035_showcase, &["showcase"], "/showcase");
test_static_route!(test_static_036_templates, &["templates"], "/templates");
test_static_route!(test_static_037_enterprise, &["enterprise"], "/enterprise");
test_static_route!(test_static_038_customers, &["customers"], "/customers");
test_static_route!(test_static_039_compare, &["compare"], "/compare");
test_static_route!(test_static_040_oss, &["oss"], "/oss");
test_static_route!(test_static_041_support, &["support"], "/support");
test_static_route!(test_static_042_legal, &["legal"], "/legal");
test_static_route!(test_static_043_cookies, &["cookies"], "/cookies");
test_static_route!(test_static_044_sitemap, &["sitemap"], "/sitemap");
test_static_route!(test_static_045_rss, &["rss"], "/rss");
test_static_route!(test_static_046_nested_team, &["company", "team"], "/company/team");
test_static_route!(test_static_047_nested_engineering, &["company", "team", "engineering"], "/company/team/engineering");
test_static_route!(test_static_048_nested_backend, &["company", "team", "engineering", "backend"], "/company/team/engineering/backend");
test_static_route!(test_static_049_hyphen_name, &["my-awesome-product-launch"], "/my-awesome-product-launch");
test_static_route!(test_static_050_underscore_name, &["api_internal_health_check"], "/api_internal_health_check");

// =========================================================================
// SUITE 2: DYNAMIC PARAMETER EXTRACTION (50 TESTS)
// =========================================================================

macro_rules! test_dynamic_param {
    ($test_name:ident, $segments:expr, $request_path:expr, $expected_key:expr, $expected_val:expr) => {
        #[test]
        fn $test_name() {
            let mut router = AppRouter::new();
            router.add_route(create_test_route($segments, false));
            let matched = router.match_route($request_path);
            assert!(matched.is_some(), "Route failed to match: {}", $request_path);
            let (_, params) = matched.unwrap();
            assert_eq!(params.get($expected_key).map(|s| s.as_str()), Some($expected_val));
        }
    };
}

test_dynamic_param!(test_dynamic_051_user_id, &["users", "[id]"], "/users/42", "id", "42");
test_dynamic_param!(test_dynamic_052_user_uuid, &["users", "[id]"], "/users/123e4567-e89b-12d3-a456-426614174000", "id", "123e4567-e89b-12d3-a456-426614174000");
test_dynamic_param!(test_dynamic_053_user_string, &["users", "[id]"], "/users/son_nguyen", "id", "son_nguyen");
test_dynamic_param!(test_dynamic_054_post_slug, &["posts", "[slug]"], "/posts/building-in-rust", "slug", "building-in-rust");
test_dynamic_param!(test_dynamic_055_product_sku, &["products", "[sku]"], "/products/SKU-998822", "sku", "SKU-998822");
test_dynamic_param!(test_dynamic_056_order_num, &["orders", "[orderId]"], "/orders/ORD_2026_001", "orderId", "ORD_2026_001");
test_dynamic_param!(test_dynamic_057_invoice_id, &["invoices", "[invoiceId]"], "/invoices/INV-8899", "invoiceId", "INV-8899");
test_dynamic_param!(test_dynamic_058_org_slug, &["orgs", "[org]"], "/orgs/nata-cloud", "org", "nata-cloud");
test_dynamic_param!(test_dynamic_059_repo_slug, &["repos", "[repo]"], "/repos/com-compiler", "repo", "com-compiler");
test_dynamic_param!(test_dynamic_060_commit_hash, &["commits", "[hash]"], "/commits/a1b2c3d4e5f6", "hash", "a1b2c3d4e5f6");
test_dynamic_param!(test_dynamic_061_branch_name, &["branches", "[branch]"], "/branches/feature-router", "branch", "feature-router");
test_dynamic_param!(test_dynamic_062_tag_version, &["tags", "[tag]"], "/tags/v1.0.0-rc1", "tag", "v1.0.0-rc1");
test_dynamic_param!(test_dynamic_063_locale_vi, &["[locale]", "about"], "/vi/about", "locale", "vi");
test_dynamic_param!(test_dynamic_064_locale_en, &["[locale]", "about"], "/en/about", "locale", "en");
test_dynamic_param!(test_dynamic_065_locale_ja, &["[locale]", "about"], "/ja/about", "locale", "ja");
test_dynamic_param!(test_dynamic_066_category, &["categories", "[category]"], "/categories/electronics", "category", "electronics");
test_dynamic_param!(test_dynamic_067_author, &["authors", "[author]"], "/authors/dan-abramov", "author", "dan-abramov");
test_dynamic_param!(test_dynamic_068_year, &["archive", "[year]"], "/archive/2026", "year", "2026");
test_dynamic_param!(test_dynamic_069_token, &["tokens", "[token]"], "/tokens/jwt_secret_token_123", "token", "jwt_secret_token_123");
test_dynamic_param!(test_dynamic_070_session, &["sessions", "[sessionId]"], "/sessions/sess_abc123", "sessionId", "sess_abc123");
test_dynamic_param!(test_dynamic_071_payment, &["payments", "[paymentId]"], "/payments/pay_992211", "paymentId", "pay_992211");
test_dynamic_param!(test_dynamic_072_subscription, &["subs", "[subId]"], "/subs/sub_pro_yearly", "subId", "sub_pro_yearly");
test_dynamic_param!(test_dynamic_073_team, &["teams", "[teamId]"], "/teams/team_core_eng", "teamId", "team_core_eng");
test_dynamic_param!(test_dynamic_074_member, &["members", "[memberId]"], "/members/mem_007", "memberId", "mem_007");
test_dynamic_param!(test_dynamic_075_file_key, &["files", "[key]"], "/files/avatar.png", "key", "avatar.png");

// Multi-param dynamic routes
#[test]
fn test_dynamic_076_two_params_org_repo() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["[org]", "[repo]"], false));
    let (_, params) = router.match_route("/nata/backend").unwrap();
    assert_eq!(params.get("org").unwrap(), "nata");
    assert_eq!(params.get("repo").unwrap(), "backend");
}

#[test]
fn test_dynamic_077_two_params_users_posts() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["users", "[userId]", "posts", "[postId]"], false));
    let (_, params) = router.match_route("/users/u10/posts/p99").unwrap();
    assert_eq!(params.get("userId").unwrap(), "u10");
    assert_eq!(params.get("postId").unwrap(), "p99");
}

#[test]
fn test_dynamic_078_three_params_shop_cat_prod_rev() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["shop", "[category]", "[productId]", "reviews", "[reviewId]"], false));
    let (_, params) = router.match_route("/shop/laptops/macbook-pro/reviews/rev-1").unwrap();
    assert_eq!(params.get("category").unwrap(), "laptops");
    assert_eq!(params.get("productId").unwrap(), "macbook-pro");
    assert_eq!(params.get("reviewId").unwrap(), "rev-1");
}

#[test]
fn test_dynamic_079_four_params_date_slug() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["blog", "[year]", "[month]", "[day]", "[slug]"], false));
    let (_, params) = router.match_route("/blog/2026/09/16/announcing-rust-dev-engine").unwrap();
    assert_eq!(params.get("year").unwrap(), "2026");
    assert_eq!(params.get("month").unwrap(), "09");
    assert_eq!(params.get("day").unwrap(), "16");
    assert_eq!(params.get("slug").unwrap(), "announcing-rust-dev-engine");
}

#[test]
fn test_dynamic_080_five_params_hierarchy() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["api", "[v]", "orgs", "[orgId]", "projects", "[projId]", "tasks", "[taskId]"], true));
    let (route, params) = router.match_route("/api/v1/orgs/my-org/projects/proj-42/tasks/task-999").unwrap();
    assert!(route.is_api);
    assert_eq!(params.get("v").unwrap(), "v1");
    assert_eq!(params.get("orgId").unwrap(), "my-org");
    assert_eq!(params.get("projId").unwrap(), "proj-42");
    assert_eq!(params.get("taskId").unwrap(), "task-999");
}

test_dynamic_param!(test_dynamic_081_locale_post, &["[lang]", "posts", "[id]"], "/fr/posts/100", "lang", "fr");
test_dynamic_param!(test_dynamic_082_tenant_dashboard, &["tenants", "[tenant]", "dashboard"], "/tenants/acme-corp/dashboard", "tenant", "acme-corp");
test_dynamic_param!(test_dynamic_083_device_metric, &["devices", "[deviceId]", "metrics"], "/devices/iot-sensor-01/metrics", "deviceId", "iot-sensor-01");
test_dynamic_param!(test_dynamic_084_contract_clause, &["contracts", "[contractId]", "clauses"], "/contracts/ct-8899/clauses", "contractId", "ct-8899");
test_dynamic_param!(test_dynamic_085_channel_message, &["channels", "[channelId]", "messages"], "/channels/general/messages", "channelId", "general");
test_dynamic_param!(test_dynamic_086_course_lesson, &["courses", "[courseId]", "lessons"], "/courses/rust-101/lessons", "courseId", "rust-101");
test_dynamic_param!(test_dynamic_087_warehouse_item, &["warehouses", "[whId]", "inventory"], "/warehouses/wh-east/inventory", "whId", "wh-east");
test_dynamic_param!(test_dynamic_088_cluster_node, &["clusters", "[clusterId]", "nodes"], "/clusters/k8s-prod/nodes", "clusterId", "k8s-prod");
test_dynamic_param!(test_dynamic_089_wallet_tx, &["wallets", "[addr]", "txs"], "/wallets/0x12345678/txs", "addr", "0x12345678");
test_dynamic_param!(test_dynamic_090_dataset_table, &["datasets", "[dsId]", "tables"], "/datasets/finance-2026/tables", "dsId", "finance-2026");
test_dynamic_param!(test_dynamic_091_artist_albums, &["artists", "[artistId]", "albums"], "/artists/taylor/albums", "artistId", "taylor");
test_dynamic_param!(test_dynamic_092_video_stream, &["videos", "[videoId]", "stream"], "/videos/vid-9900/stream", "videoId", "vid-9900");
test_dynamic_param!(test_dynamic_093_game_match, &["games", "[gameId]", "matches"], "/games/cs2/matches", "gameId", "cs2");
test_dynamic_param!(test_dynamic_094_flight_seat, &["flights", "[flightNo]", "seats"], "/flights/VN204/seats", "flightNo", "VN204");
test_dynamic_param!(test_dynamic_095_hotel_room, &["hotels", "[hotelId]", "rooms"], "/hotels/marriott-hcm/rooms", "hotelId", "marriott-hcm");
test_dynamic_param!(test_dynamic_096_recipe_step, &["recipes", "[recipeId]", "steps"], "/recipes/pho-bo/steps", "recipeId", "pho-bo");
test_dynamic_param!(test_dynamic_097_podcast_ep, &["podcasts", "[podId]", "episodes"], "/podcasts/lex-fridman/episodes", "podId", "lex-fridman");
test_dynamic_param!(test_dynamic_098_book_chapter, &["books", "[bookId]", "chapters"], "/books/the-rust-book/chapters", "bookId", "the-rust-book");
test_dynamic_param!(test_dynamic_099_car_model, &["brands", "[brandId]", "models"], "/brands/porsche/models", "brandId", "porsche");
test_dynamic_param!(test_dynamic_100_university_dept, &["schools", "[schoolId]", "departments"], "/schools/mit/departments", "schoolId", "mit");

// =========================================================================
// SUITE 3: CATCH-ALL [...slug] ROUTES (40 TESTS)
// =========================================================================

macro_rules! test_catch_all {
    ($test_name:ident, $segments:expr, $request_path:expr, $expected_key:expr, $expected_val:expr) => {
        #[test]
        fn $test_name() {
            let mut router = AppRouter::new();
            router.add_route(create_test_route($segments, false));
            let matched = router.match_route($request_path);
            assert!(matched.is_some(), "Catch-all failed to match: {}", $request_path);
            let (_, params) = matched.unwrap();
            assert_eq!(params.get($expected_key).map(|s| s.as_str()), Some($expected_val));
        }
    };
}

test_catch_all!(test_catchall_101_single, &["docs", "[...slug]"], "/docs/getting-started", "slug", "getting-started");
test_catch_all!(test_catchall_102_double, &["docs", "[...slug]"], "/docs/api/v1", "slug", "api/v1");
test_catch_all!(test_catchall_103_triple, &["docs", "[...slug]"], "/docs/guides/auth/jwt", "slug", "guides/auth/jwt");
test_catch_all!(test_catchall_104_quadruple, &["docs", "[...slug]"], "/docs/arch/compiler/rust/ast", "slug", "arch/compiler/rust/ast");
test_catch_all!(test_catchall_105_quintuple, &["docs", "[...slug]"], "/docs/a/b/c/d/e", "slug", "a/b/c/d/e");
test_catch_all!(test_catchall_106_files_deep, &["files", "[...path]"], "/files/usr/local/bin/rustc", "path", "usr/local/bin/rustc");
test_catch_all!(test_catchall_107_media_path, &["media", "[...filepath]"], "/media/uploads/2026/09/banner.jpg", "filepath", "uploads/2026/09/banner.jpg");
test_catch_all!(test_catchall_108_storage_blob, &["storage", "[...blob]"], "/storage/bucket1/folder/sub/data.json", "blob", "bucket1/folder/sub/data.json");
test_catch_all!(test_catchall_109_wiki_page, &["wiki", "[...entry]"], "/wiki/Vietnam/History/Dynasties", "entry", "Vietnam/History/Dynasties");
test_catch_all!(test_catchall_110_shop_tree, &["shop", "[...categoryPath]"], "/shop/electronics/computers/laptops/accessories", "categoryPath", "electronics/computers/laptops/accessories");
test_catch_all!(test_catchall_111_repo_tree, &["repos", "[repo]", "tree", "[...path]"], "/repos/compiler-rs/tree/src/router/mod.rs", "path", "src/router/mod.rs");
test_catch_all!(test_catchall_112_repo_blob, &["repos", "[repo]", "blob", "[...path]"], "/repos/nata/blob/main/Cargo.toml", "path", "main/Cargo.toml");
test_catch_all!(test_catchall_113_workspace_files, &["ws", "[wsId]", "files", "[...filePath]"], "/ws/ws_1/files/package.json", "filePath", "package.json");
test_catch_all!(test_catchall_114_workspace_deep, &["ws", "[wsId]", "files", "[...filePath]"], "/ws/ws_1/files/src/app/layout.tsx", "filePath", "src/app/layout.tsx");
test_catch_all!(test_catchall_115_cdn_assets, &["cdn", "[...asset]"], "/cdn/fonts/inter/inter-var.woff2", "asset", "fonts/inter/inter-var.woff2");
test_catch_all!(test_catchall_116_proxy_path, &["proxy", "[...target]"], "/proxy/api.github.com/repos/google", "target", "api.github.com/repos/google");
test_catch_all!(test_catchall_117_logs_service, &["logs", "[service]", "[...filter]"], "/logs/auth-svc/errors/critical/today", "filter", "errors/critical/today");
test_catch_all!(test_catchall_118_git_raw, &["raw", "[...refPath]"], "/raw/heads/main/README.md", "refPath", "heads/main/README.md");
test_catch_all!(test_catchall_119_admin_audit, &["admin", "audit", "[...filters]"], "/admin/audit/users/logins/failed", "filters", "users/logins/failed");
test_catch_all!(test_catchall_120_explorer, &["explorer", "[...node]"], "/explorer/asia/vietnam/hanoi/ba_dinh", "node", "asia/vietnam/hanoi/ba_dinh");
test_catch_all!(test_catchall_121_cms_pages, &["cms", "[...pagePath]"], "/cms/landing/enterprise/solutions", "pagePath", "landing/enterprise/solutions");
test_catch_all!(test_catchall_122_i18n_docs, &["[lang]", "docs", "[...slug]"], "/vi/docs/installation/quickstart", "slug", "installation/quickstart");
test_catch_all!(test_catchall_123_s3_browse, &["s3", "[bucket]", "[...key]"], "/s3/my-bucket/photos/2026/beach.png", "key", "photos/2026/beach.png");
test_catch_all!(test_catchall_124_reports_nested, &["reports", "[...query]"], "/reports/q3/sales/regional/emea", "query", "q3/sales/regional/emea");
test_catch_all!(test_catchall_125_archive_dates, &["archive", "[...dateRange]"], "/archive/2026/09/16", "dateRange", "2026/09/16");
test_catch_all!(test_catchall_126_sdk_api, &["sdk", "[lang]", "[...method]"], "/sdk/rust/client/authenticate", "method", "client/authenticate");
test_catch_all!(test_catchall_127_metrics_tags, &["metrics", "[...tags]"], "/metrics/cpu/host01/core0/temp", "tags", "cpu/host01/core0/temp");
test_catch_all!(test_catchall_128_fs_symlink, &["fs", "[...sym]"], "/fs/etc/nginx/sites-enabled/default", "sym", "etc/nginx/sites-enabled/default");
test_catch_all!(test_catchall_129_schemas, &["schemas", "[...versioned]"], "/schemas/v2/events/user_created.json", "versioned", "v2/events/user_created.json");
test_catch_all!(test_catchall_130_routes_dump, &["debug", "routes", "[...dump]"], "/debug/routes/all/active", "dump", "all/active");
test_catch_all!(test_catchall_131_graphql_alias, &["gql", "[...alias]"], "/gql/queries/user_profile", "alias", "queries/user_profile");
test_catch_all!(test_catchall_132_trace_spans, &["trace", "[...spans]"], "/trace/span_1/span_2/span_3", "spans", "span_1/span_2/span_3");
test_catch_all!(test_catchall_133_helm_charts, &["charts", "[...chartPath]"], "/charts/stable/redis/values.yaml", "chartPath", "stable/redis/values.yaml");
test_catch_all!(test_catchall_134_k8s_manifests, &["k8s", "[...manifest]"], "/k8s/deployments/apps/backend.yaml", "manifest", "deployments/apps/backend.yaml");
test_catch_all!(test_catchall_135_terraform_state, &["tf", "[...stateKey]"], "/tf/env/prod/vpc/terraform.tfstate", "stateKey", "env/prod/vpc/terraform.tfstate");
test_catch_all!(test_catchall_136_dns_records, &["dns", "[...subdomain]"], "/dns/app/internal/corp", "subdomain", "app/internal/corp");
test_catch_all!(test_catchall_137_docker_layers, &["docker", "[...layer]"], "/docker/v2/blobs/sha256/12345", "layer", "v2/blobs/sha256/12345");
test_catch_all!(test_catchall_138_npm_package_tar, &["npm", "[...pkg]"], "/npm/@com.ai.vn/cli/-/cli-1.0.0.tgz", "pkg", "@com.ai.vn/cli/-/cli-1.0.0.tgz");
test_catch_all!(test_catchall_139_cargo_crate, &["crates", "[...cratePath]"], "/crates/axum/0.8.0/download", "cratePath", "axum/0.8.0/download");
test_catch_all!(test_catchall_140_pypi_wheel, &["pypi", "[...whl]"], "/pypi/packages/torch/torch-2.0-cp311.whl", "whl", "packages/torch/torch-2.0-cp311.whl");

// =========================================================================
// SUITE 4: OPTIONAL CATCH-ALL [[...slug]] ROUTES (30 TESTS)
// =========================================================================

#[test]
fn test_opt_catchall_141_base_match() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["blog", "[[...slug]]"], false));
    let (route, params) = router.match_route("/blog").unwrap();
    assert_eq!(route.pattern, "/blog/[[...slug]]");
    assert!(params.is_empty() || params.get("slug").unwrap().is_empty());
}

#[test]
fn test_opt_catchall_142_single_segment() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["blog", "[[...slug]]"], false));
    let (_, params) = router.match_route("/blog/hello-world").unwrap();
    assert_eq!(params.get("slug").unwrap(), "hello-world");
}

#[test]
fn test_opt_catchall_143_multi_segment() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["blog", "[[...slug]]"], false));
    let (_, params) = router.match_route("/blog/2026/09/hello-world").unwrap();
    assert_eq!(params.get("slug").unwrap(), "2026/09/hello-world");
}

#[test]
fn test_opt_catchall_144_store_base() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["store", "[[...filter]]"], false));
    assert!(router.match_route("/store").is_some());
}

#[test]
fn test_opt_catchall_145_store_filter() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["store", "[[...filter]]"], false));
    let (_, params) = router.match_route("/store/brand/apple/color/silver").unwrap();
    assert_eq!(params.get("filter").unwrap(), "brand/apple/color/silver");
}

macro_rules! test_opt_catch_all_suite {
    ($test_name:ident, $prefix:expr, $param:expr, $req:expr, $expected_val:expr) => {
        #[test]
        fn $test_name() {
            let mut router = AppRouter::new();
            let seg = format!("[[...{}]]", $param);
            router.add_route(create_test_route(&[$prefix, &seg], false));
            let matched = router.match_route($req);
            assert!(matched.is_some(), "Optional catch-all failed for {}", $req);
            let (_, params) = matched.unwrap();
            if let Some(exp) = $expected_val {
                assert_eq!(params.get($param).map(|s| s.as_str()), Some(exp));
            }
        }
    };
}

test_opt_catch_all_suite!(test_opt_146_docs_empty, "docs", "path", "/docs", None::<&str>);
test_opt_catch_all_suite!(test_opt_147_docs_one, "docs", "path", "/docs/intro", Some("intro"));
test_opt_catch_all_suite!(test_opt_148_docs_two, "docs", "path", "/docs/intro/cli", Some("intro/cli"));
test_opt_catch_all_suite!(test_opt_149_catalog_empty, "catalog", "cat", "/catalog", None::<&str>);
test_opt_catch_all_suite!(test_opt_150_catalog_deep, "catalog", "cat", "/catalog/men/shoes/running", Some("men/shoes/running"));
test_opt_catch_all_suite!(test_opt_151_reports_empty, "reports", "rep", "/reports", None::<&str>);
test_opt_catch_all_suite!(test_opt_152_reports_range, "reports", "rep", "/reports/2026/q3", Some("2026/q3"));
test_opt_catch_all_suite!(test_opt_153_events_empty, "events", "ev", "/events", None::<&str>);
test_opt_catch_all_suite!(test_opt_154_events_tagged, "events", "ev", "/events/rust/asia/2026", Some("rust/asia/2026"));
test_opt_catch_all_suite!(test_opt_155_feed_empty, "feed", "channel", "/feed", None::<&str>);
test_opt_catch_all_suite!(test_opt_156_feed_channel, "feed", "channel", "/feed/tech/ai", Some("tech/ai"));
test_opt_catch_all_suite!(test_opt_157_search_empty, "search", "query", "/search", None::<&str>);
test_opt_catch_all_suite!(test_opt_158_search_tags, "search", "query", "/search/tag/rust/topic/compiler", Some("tag/rust/topic/compiler"));
test_opt_catch_all_suite!(test_opt_159_logs_empty, "logs", "stream", "/logs", None::<&str>);
test_opt_catch_all_suite!(test_opt_160_logs_stream, "logs", "stream", "/logs/prod/us-east-1", Some("prod/us-east-1"));
test_opt_catch_all_suite!(test_opt_161_matrix_empty, "matrix", "cell", "/matrix", None::<&str>);
test_opt_catch_all_suite!(test_opt_162_matrix_cell, "matrix", "cell", "/matrix/row1/col2", Some("row1/col2"));
test_opt_catch_all_suite!(test_opt_163_browse_empty, "browse", "b", "/browse", None::<&str>);
test_opt_catch_all_suite!(test_opt_164_browse_node, "browse", "b", "/browse/genres/rock/classic", Some("genres/rock/classic"));
test_opt_catch_all_suite!(test_opt_165_vault_empty, "vault", "v", "/vault", None::<&str>);
test_opt_catch_all_suite!(test_opt_166_vault_path, "vault", "v", "/vault/secrets/api_keys", Some("secrets/api_keys"));
test_opt_catch_all_suite!(test_opt_167_assets_empty, "assets", "a", "/assets", None::<&str>);
test_opt_catch_all_suite!(test_opt_168_assets_img, "assets", "a", "/assets/images/logo.svg", Some("images/logo.svg"));
test_opt_catch_all_suite!(test_opt_169_kb_empty, "kb", "article", "/kb", None::<&str>);
test_opt_catch_all_suite!(test_opt_170_kb_article, "kb", "article", "/kb/billing/refund-policy", Some("billing/refund-policy"));

// =========================================================================
// SUITE 5: ROUTE GROUPS (group) & PARALLEL SLOTS @slot (25 TESTS)
// =========================================================================

macro_rules! test_route_group {
    ($test_name:ident, $segments:expr, $expected_pattern:expr, $request_path:expr) => {
        #[test]
        fn $test_name() {
            let mut router = AppRouter::new();
            let route = create_test_route($segments, false);
            assert_eq!(route.pattern, $expected_pattern);
            router.add_route(route);
            assert!(router.match_route($request_path).is_some());
        }
    };
}

test_route_group!(test_group_171_marketing_about, &["(marketing)", "about"], "/about", "/about");
test_route_group!(test_group_172_marketing_contact, &["(marketing)", "contact"], "/contact", "/contact");
test_route_group!(test_group_173_marketing_pricing, &["(marketing)", "pricing"], "/pricing", "/pricing");
test_route_group!(test_group_174_auth_login, &["(auth)", "login"], "/login", "/login");
test_route_group!(test_group_175_auth_register, &["(auth)", "register"], "/register", "/register");
test_route_group!(test_group_176_auth_forgot, &["(auth)", "forgot-password"], "/forgot-password", "/forgot-password");
test_route_group!(test_group_177_dashboard_home, &["(dashboard)", "dashboard"], "/dashboard", "/dashboard");
test_route_group!(test_group_178_dashboard_settings, &["(dashboard)", "settings"], "/settings", "/settings");
test_route_group!(test_group_179_dashboard_profile, &["(dashboard)", "profile"], "/profile", "/profile");
test_route_group!(test_group_180_double_group, &["(admin)", "(finance)", "invoices"], "/invoices", "/invoices");
test_route_group!(test_group_181_group_with_dynamic, &["(admin)", "users", "[id]"], "/users/[id]", "/users/user-123");
test_route_group!(test_group_182_group_dynamic_nested, &["(shop)", "categories", "[category]", "items", "[itemId]"], "/categories/[category]/items/[itemId]", "/categories/shoes/items/nike-air");
test_route_group!(test_group_183_group_with_catchall, &["(docs)", "docs", "[...slug]"], "/docs/[...slug]", "/docs/api/endpoints");
test_route_group!(test_group_184_group_root, &["(site)"], "/", "/");
test_route_group!(test_group_185_group_landing, &["(marketing)", "(v2)", "landing"], "/landing", "/landing");
test_route_group!(test_group_186_group_checkout, &["(checkout)", "cart"], "/cart", "/cart");
test_route_group!(test_group_187_group_payment, &["(checkout)", "payment"], "/payment", "/payment");
test_route_group!(test_group_188_group_success, &["(checkout)", "success"], "/success", "/success");
test_route_group!(test_group_189_group_onboarding, &["(onboarding)", "step-1"], "/step-1", "/step-1");
test_route_group!(test_group_190_group_onboarding2, &["(onboarding)", "step-2"], "/step-2", "/step-2");

#[test]
fn test_slot_191_parallel_modal() {
    let segs = SegmentParser::parse_segments(&["@modal", "login"]);
    assert_eq!(segs[0].segment_type, SegmentType::ParallelSlot("modal".to_string()));
    assert!(!segs[0].is_url_segment());
    assert_eq!(segs[1].segment_type, SegmentType::Static("login".to_string()));
}

#[test]
fn test_slot_192_parallel_auth() {
    let segs = SegmentParser::parse_segments(&["@auth", "login"]);
    assert_eq!(segs[0].segment_type, SegmentType::ParallelSlot("auth".to_string()));
}

#[test]
fn test_slot_193_parallel_analytics() {
    let segs = SegmentParser::parse_segments(&["dashboard", "@analytics", "page"]);
    assert_eq!(segs[1].segment_type, SegmentType::ParallelSlot("analytics".to_string()));
}

#[test]
fn test_slot_194_parallel_sidebar() {
    let segs = SegmentParser::parse_segments(&["@sidebar", "menu"]);
    assert_eq!(segs[0].segment_type, SegmentType::ParallelSlot("sidebar".to_string()));
}

#[test]
fn test_slot_195_intercepting_route() {
    let segs = SegmentParser::parse_segments(&["feed", "(..)photo", "[id]"]);
    assert!(matches!(segs[1].segment_type, SegmentType::Intercepting(_)));
}

// =========================================================================
// SUITE 6: URL NORMALIZATION, QUERY STRINGS, ENCODING & PRECEDENCE (40+ TESTS)
// =========================================================================

#[test]
fn test_norm_196_trailing_slash() {
    let (clean, _) = RouteMatcher::normalize_url("/about/");
    assert_eq!(clean, "/about");
}

#[test]
fn test_norm_197_multiple_slashes() {
    let (clean, _) = RouteMatcher::normalize_url("///users////123///");
    assert_eq!(clean, "/users/123");
}

#[test]
fn test_norm_198_root_slash() {
    let (clean, _) = RouteMatcher::normalize_url("///");
    assert_eq!(clean, "/");
}

#[test]
fn test_norm_199_query_single() {
    let (clean, query) = RouteMatcher::normalize_url("/search?q=rust");
    assert_eq!(clean, "/search");
    assert_eq!(query.get("q").unwrap(), "rust");
}

#[test]
fn test_norm_200_query_multi() {
    let (clean, query) = RouteMatcher::normalize_url("/products?cat=electronics&sort=asc&page=2");
    assert_eq!(clean, "/products");
    assert_eq!(query.get("cat").unwrap(), "electronics");
    assert_eq!(query.get("sort").unwrap(), "asc");
    assert_eq!(query.get("page").unwrap(), "2");
}

#[test]
fn test_norm_201_hash_strip() {
    let (clean, _) = RouteMatcher::normalize_url("/docs/intro#getting-started");
    assert_eq!(clean, "/docs/intro");
}

#[test]
fn test_norm_202_hash_and_query() {
    let (clean, query) = RouteMatcher::normalize_url("/docs?lang=vi#section-2");
    assert_eq!(clean, "/docs");
    assert_eq!(query.get("lang").unwrap(), "vi");
}

#[test]
fn test_norm_203_url_decode_spaces() {
    let decoded = RouteMatcher::url_decode("hello%20world");
    assert_eq!(decoded, "hello world");
}

#[test]
fn test_norm_204_url_decode_plus() {
    let decoded = RouteMatcher::url_decode("hello+world");
    assert_eq!(decoded, "hello world");
}

#[test]
fn test_norm_205_url_decode_hex() {
    let decoded = RouteMatcher::url_decode("%24%40%21");
    assert_eq!(decoded, "$@!");
}

#[test]
fn test_norm_206_request_match_with_query() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["search"], false));
    let match_res = router.match_request("/search?q=nextjs&filter=active").unwrap();
    assert_eq!(match_res.matched_path, "/search");
    assert_eq!(match_res.query.get("q").unwrap(), "nextjs");
    assert_eq!(match_res.query.get("filter").unwrap(), "active");
}

#[test]
fn test_norm_207_request_match_dynamic_and_query() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["users", "[id]"], false));
    let match_res = router.match_request("/users/user-42?tab=billing&mode=dark").unwrap();
    assert_eq!(match_res.params.get("id").unwrap(), "user-42");
    assert_eq!(match_res.query.get("tab").unwrap(), "billing");
    assert_eq!(match_res.query.get("mode").unwrap(), "dark");
}

#[test]
fn test_precedence_208_static_over_dynamic() {
    let mut router = AppRouter::new();
    // Add dynamic route first
    router.add_route(create_test_route(&["posts", "[id]"], false));
    // Add static route second
    router.add_route(create_test_route(&["posts", "new"], false));

    let (route, params) = router.match_route("/posts/new").unwrap();
    assert_eq!(route.pattern, "/posts/new");
    assert!(params.is_empty());
}

#[test]
fn test_precedence_209_dynamic_over_catchall() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["docs", "[...slug]"], false));
    router.add_route(create_test_route(&["docs", "[id]"], false));

    let (route, params) = router.match_route("/docs/quickstart").unwrap();
    assert_eq!(route.pattern, "/docs/[id]");
    assert_eq!(params.get("id").unwrap(), "quickstart");
}

#[test]
fn test_precedence_210_catchall_multi_segment() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["docs", "[id]"], false));
    router.add_route(create_test_route(&["docs", "[...slug]"], false));

    let (route, params) = router.match_route("/docs/api/v1/auth").unwrap();
    assert_eq!(route.pattern, "/docs/[...slug]");
    assert_eq!(params.get("slug").unwrap(), "api/v1/auth");
}

#[test]
fn test_precedence_211_static_over_catchall() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["files", "[...path]"], false));
    router.add_route(create_test_route(&["files", "config.json"], false));

    let (route, _) = router.match_route("/files/config.json").unwrap();
    assert_eq!(route.pattern, "/files/config.json");
}

#[test]
fn test_404_212_unknown_route() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["about"], false));
    router.add_route(create_test_route(&["contact"], false));
    assert!(router.match_route("/non-existent").is_none());
}

#[test]
fn test_404_213_prefix_mismatch() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["users", "[id]"], false));
    assert!(router.match_route("/user/123").is_none());
}

#[test]
fn test_404_214_deep_mismatch() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["users", "[id]"], false));
    assert!(router.match_route("/users/123/extra").is_none());
}

#[test]
fn test_unicode_215_vietnamese() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["sản-phẩm", "[tên]"], false));
    let (_, params) = router.match_route("/sản-phẩm/áo-thun-nam").unwrap();
    assert_eq!(params.get("tên").unwrap(), "áo-thun-nam");
}

#[test]
fn test_unicode_216_japanese() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["ユーザー", "[id]"], false));
    let (_, params) = router.match_route("/ユーザー/太郎").unwrap();
    assert_eq!(params.get("id").unwrap(), "太郎");
}

#[test]
fn test_unicode_217_encoded_vietnamese() {
    let (clean, _) = RouteMatcher::normalize_url("/s%E1%BA%A3n-ph%E1%BA%A9m");
    assert_eq!(clean, "/sản-phẩm");
}

#[test]
fn test_api_218_route_detection() {
    let route = create_test_route(&["api", "v1", "users"], true);
    assert!(route.is_api);
    assert_eq!(route.kind, RouteKind::Api);
    assert_eq!(route.pattern, "/api/v1/users");
}

#[test]
fn test_api_219_dynamic_endpoint() {
    let mut router = AppRouter::new();
    router.add_route(create_test_route(&["api", "users", "[id]"], true));
    let (route, params) = router.match_route("/api/users/9988").unwrap();
    assert!(route.is_api);
    assert_eq!(params.get("id").unwrap(), "9988");
}

#[test]
fn test_router_220_len_and_empty() {
    let mut router = AppRouter::new();
    assert!(router.is_empty());
    assert_eq!(router.len(), 0);
    router.add_route(create_test_route(&["home"], false));
    assert!(!router.is_empty());
    assert_eq!(router.len(), 1);
}

#[test]
fn test_layout_221_inheritance_chain() {
    let route = create_test_route(&["dashboard", "settings"], false);
    assert_eq!(route.layout_files.len(), 1);
    assert_eq!(route.layout_files[0], PathBuf::from("app/layout.tsx"));
}

#[test]
fn test_score_222_static_higher_than_dynamic() {
    let static_seg = SegmentParser::parse("about");
    let dynamic_seg = SegmentParser::parse("[id]");
    assert!(static_seg.score() > dynamic_seg.score());
}

#[test]
fn test_score_223_dynamic_higher_than_catchall() {
    let dynamic_seg = SegmentParser::parse("[id]");
    let catchall_seg = SegmentParser::parse("[...slug]");
    assert!(dynamic_seg.score() > catchall_seg.score());
}

#[test]
fn test_score_224_optional_catchall_higher_than_catchall() {
    let opt_seg = SegmentParser::parse("[[...slug]]");
    let catchall_seg = SegmentParser::parse("[...slug]");
    assert!(opt_seg.score() > catchall_seg.score());
}

#[test]
fn test_parser_225_plain_text() {
    let seg = SegmentParser::parse("products");
    assert_eq!(seg.segment_type, SegmentType::Static("products".to_string()));
}

#[test]
fn test_parser_226_dynamic_brackets() {
    let seg = SegmentParser::parse("[customId]");
    assert_eq!(seg.segment_type, SegmentType::Dynamic("customId".to_string()));
}

#[test]
fn test_parser_227_catchall_dots() {
    let seg = SegmentParser::parse("[...all]");
    assert_eq!(seg.segment_type, SegmentType::CatchAll("all".to_string()));
}

#[test]
fn test_parser_228_opt_catchall_double_brackets() {
    let seg = SegmentParser::parse("[[...filters]]");
    assert_eq!(seg.segment_type, SegmentType::OptionalCatchAll("filters".to_string()));
}

#[test]
fn test_parser_229_route_group_parens() {
    let seg = SegmentParser::parse("(auth)");
    assert_eq!(seg.segment_type, SegmentType::RouteGroup("auth".to_string()));
}

#[test]
fn test_parser_230_parallel_slot_at() {
    let seg = SegmentParser::parse("@modal");
    assert_eq!(seg.segment_type, SegmentType::ParallelSlot("modal".to_string()));
}

#[test]
fn test_parser_231_intercepting_dot() {
    let seg = SegmentParser::parse("(.)photo");
    assert_eq!(seg.segment_type, SegmentType::Intercepting("(.)photo".to_string()));
}

#[test]
fn test_parser_232_intercepting_dotdot() {
    let seg = SegmentParser::parse("(..)feed");
    assert_eq!(seg.segment_type, SegmentType::Intercepting("(..)feed".to_string()));
}

#[test]
fn test_parser_233_intercepting_dotdotdot() {
    let seg = SegmentParser::parse("(...)root");
    assert_eq!(seg.segment_type, SegmentType::Intercepting("(...)root".to_string()));
}

#[test]
fn test_matcher_234_empty_query() {
    let (path, query) = RouteMatcher::normalize_url("/blog?");
    assert_eq!(path, "/blog");
    assert!(query.is_empty());
}

#[test]
fn test_matcher_235_query_without_value() {
    let (path, query) = RouteMatcher::normalize_url("/search?debug&verbose");
    assert_eq!(path, "/search");
    assert_eq!(query.get("debug").unwrap(), "");
    assert_eq!(query.get("verbose").unwrap(), "");
}
