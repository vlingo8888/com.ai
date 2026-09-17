use compiler_rs::rpc::{format_module_path, RpcExecutor, RpcPayload, RpcResponse};
use std::fs;
use std::path::PathBuf;

fn setup_test_project(test_name: &str) -> PathBuf {
    let tmp = std::env::temp_dir().join(format!("nata_rpc_test_{}_{}", test_name, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    tmp
}

// -----------------------------------------------------------------------------
// SECTION 1: Module Path Resolution Tests (50 testcases)
// -----------------------------------------------------------------------------

#[test]
fn test_001_to_050_module_path_resolution() {
    let tmp = setup_test_project("mod_resolve");

    // Create various directory structures and files
    fs::create_dir_all(tmp.join("modules/auth")).unwrap();
    fs::create_dir_all(tmp.join("src/modules/users")).unwrap();
    fs::create_dir_all(tmp.join("actions/billing")).unwrap();
    fs::create_dir_all(tmp.join("src/actions/settings")).unwrap();
    fs::create_dir_all(tmp.join("app/api/orders")).unwrap();
    fs::create_dir_all(tmp.join("src/app/api/products")).unwrap();
    fs::create_dir_all(tmp.join("nested/deep/service")).unwrap();

    // Create sample files
    fs::write(tmp.join("root_action.ts"), "export default () => 1;").unwrap();
    fs::write(tmp.join("root_util.js"), "module.exports = 2;").unwrap();
    fs::write(tmp.join("root_component.tsx"), "export default () => null;").unwrap();
    fs::write(tmp.join("root_esm.mjs"), "export default 3;").unwrap();
    fs::write(tmp.join("modules/auth/login.ts"), "export default () => 'login';").unwrap();
    fs::write(tmp.join("modules/auth/index.ts"), "export default () => 'auth_index';").unwrap();
    fs::write(tmp.join("src/modules/users/profile.ts"), "export default () => 'profile';").unwrap();
    fs::write(tmp.join("src/modules/users/index.tsx"), "export default () => 'users_index';").unwrap();
    fs::write(tmp.join("actions/billing/invoice.ts"), "export default () => 'invoice';").unwrap();
    fs::write(tmp.join("src/actions/settings/general.ts"), "export default () => 'general';").unwrap();
    fs::write(tmp.join("app/api/orders/route.ts"), "export default () => 'orders';").unwrap();
    fs::write(tmp.join("src/app/api/products/route.ts"), "export default () => 'products';").unwrap();
    fs::write(tmp.join("nested/deep/service/item.service.ts"), "export default () => 'item';").unwrap();

    let test_cases: Vec<(&str, bool, &str)> = vec![
        // 1-10: Direct root files
        ("root_action", true, "root_action.ts"),
        ("./root_action", true, "root_action.ts"),
        ("@/root_action", true, "root_action.ts"),
        ("~/root_action", true, "root_action.ts"),
        ("/root_action", true, "root_action.ts"),
        ("root_action.ts", true, "root_action.ts"),
        ("root_util", true, "root_util.js"),
        ("root_util.js", true, "root_util.js"),
        ("root_component", true, "root_component.tsx"),
        ("root_esm", true, "root_esm.mjs"),
        
        // 11-20: modules/ folder resolution
        ("modules/auth/login", true, "modules/auth/login.ts"),
        ("@/modules/auth/login", true, "modules/auth/login.ts"),
        ("auth/login", true, "modules/auth/login.ts"),
        ("./auth/login", true, "modules/auth/login.ts"),
        ("modules/auth", true, "modules/auth/index.ts"),
        ("auth", true, "modules/auth/index.ts"),
        ("@/auth", true, "modules/auth/index.ts"),
        ("modules/auth/index.ts", true, "modules/auth/index.ts"),
        ("auth/index", true, "modules/auth/index.ts"),
        ("./modules/auth/login.ts", true, "modules/auth/login.ts"),

        // 21-30: src/modules/ folder resolution
        ("src/modules/users/profile", true, "src/modules/users/profile.ts"),
        ("modules/users/profile", true, "src/modules/users/profile.ts"),
        ("users/profile", true, "src/modules/users/profile.ts"),
        ("@/users/profile", true, "src/modules/users/profile.ts"),
        ("~/users/profile", true, "src/modules/users/profile.ts"),
        ("src/modules/users", true, "src/modules/users/index.tsx"),
        ("users", true, "src/modules/users/index.tsx"),
        ("@/modules/users", true, "src/modules/users/index.tsx"),
        ("users/index", true, "src/modules/users/index.tsx"),
        ("src/modules/users/index.tsx", true, "src/modules/users/index.tsx"),

        // 31-40: actions/ and src/actions/ resolution
        ("actions/billing/invoice", true, "actions/billing/invoice.ts"),
        ("billing/invoice", true, "actions/billing/invoice.ts"),
        ("@/actions/billing/invoice", true, "actions/billing/invoice.ts"),
        ("src/actions/settings/general", true, "src/actions/settings/general.ts"),
        ("actions/settings/general", true, "src/actions/settings/general.ts"),
        ("settings/general", true, "src/actions/settings/general.ts"),
        ("@/settings/general", true, "src/actions/settings/general.ts"),
        ("nested/deep/service/item.service", true, "nested/deep/service/item.service.ts"),
        ("@/nested/deep/service/item.service.ts", true, "nested/deep/service/item.service.ts"),
        ("./nested/deep/service/item.service", true, "nested/deep/service/item.service.ts"),

        // 41-50: app/ routes and non-existent edge cases
        ("app/api/orders/route", true, "app/api/orders/route.ts"),
        ("api/orders/route", true, "app/api/orders/route.ts"),
        ("src/app/api/products/route", true, "src/app/api/products/route.ts"),
        ("api/products/route", true, "src/app/api/products/route.ts"),
        ("non_existent_file", false, ""),
        ("modules/unknown_module", false, ""),
        ("src/missing_sub/action", false, ""),
        ("@/invalid/path/file", false, ""),
        ("./does_not_exist.ts", false, ""),
        ("", false, ""),
    ];

    assert_eq!(test_cases.len(), 50, "Section 1 must contain exactly 50 testcases");

    for (idx, (input, should_succeed, expected_rel)) in test_cases.iter().enumerate() {
        let result = RpcExecutor::resolve_module_path(&tmp, input);
        if *should_succeed {
            assert!(
                result.is_ok(),
                "Testcase #{:03} failed: expected {:?} to resolve successfully, got {:?}",
                idx + 1,
                input,
                result
            );
            let resolved = result.unwrap();
            let expected_path = tmp.join(expected_rel);
            assert_eq!(
                resolved, expected_path,
                "Testcase #{:03} path mismatch for input {:?}",
                idx + 1,
                input
            );
        } else {
            assert!(
                result.is_err(),
                "Testcase #{:03} failed: expected {:?} to return error, but got {:?}",
                idx + 1,
                input,
                result
            );
        }
    }

    let _ = fs::remove_dir_all(&tmp);
}

// -----------------------------------------------------------------------------
// SECTION 2: Module Display Formatting Tests (30 testcases)
// -----------------------------------------------------------------------------

#[test]
fn test_051_to_080_format_module_path() {
    let test_cases = vec![
        ("modules/auth/login.ts", "@/modules/auth/login"),
        ("modules/auth/login.tsx", "@/modules/auth/login"),
        ("modules/auth/login.js", "@/modules/auth/login"),
        ("modules/auth/login.mjs", "@/modules/auth/login"),
        ("modules/users.ts", "@/modules/users"),
        ("modules/billing/invoice", "@/modules/billing/invoice"),
        ("@/modules/auth", "@/modules/auth"),
        ("@/custom/action", "@/custom/action"),
        ("@/components/ui/button", "@/components/ui/button"),
        ("auth/login.ts", "@/modules/auth/login"),
        ("users/service.ts", "@/modules/users/service"),
        ("billing/payment.tsx", "@/modules/billing/payment"),
        ("posts/create.js", "@/modules/posts/create"),
        ("comments/list.mjs", "@/modules/comments/list"),
        ("orders", "@/modules/orders"),
        ("products/detail", "@/modules/products/detail"),
        ("categories/tree.ts", "@/modules/categories/tree"),
        ("inventory/stock", "@/modules/inventory/stock"),
        ("notifications/send.tsx", "@/modules/notifications/send"),
        ("analytics/track.js", "@/modules/analytics/track"),
        ("settings/profile.ts", "@/modules/settings/profile"),
        ("teams/members.ts", "@/modules/teams/members"),
        ("integrations/webhook.mjs", "@/modules/integrations/webhook"),
        ("ai/prompt.ts", "@/modules/ai/prompt"),
        ("storage/upload.tsx", "@/modules/storage/upload"),
        ("auth/session.js", "@/modules/auth/session"),
        ("reports/export.ts", "@/modules/reports/export"),
        ("feedbacks/submit", "@/modules/feedbacks/submit"),
        ("search/query.ts", "@/modules/search/query"),
        ("system/health.ts", "@/modules/system/health"),
    ];

    assert_eq!(test_cases.len(), 30, "Section 2 must contain exactly 30 testcases");

    for (idx, (input, expected)) in test_cases.iter().enumerate() {
        let output = format_module_path(input);
        assert_eq!(
            output, *expected,
            "Testcase #{:03} format_module_path({:?}) mismatch",
            idx + 51,
            input
        );
    }
}

// -----------------------------------------------------------------------------
// SECTION 3: End-to-End RPC Execution & Response Unwrapping Tests (60 testcases)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_081_to_140_e2e_rpc_execution_and_response_unwrapping() {
    let tmp = setup_test_project("e2e_rpc");

    // 1. Module with Response.json() and new Response()
    fs::write(
        tmp.join("responses.ts"),
        r#"
export async function jsonResponse() {
    return Response.json({ success: true, count: 42, tags: ["rust", "bun"] });
}

export async function customResponse() {
    return new Response(JSON.stringify({ message: "hello world", code: 200 }), {
        headers: { "Content-Type": "application/json" }
    });
}

export async function textResponse() {
    return new Response("plain_text_data", {
        headers: { "Content-Type": "text/plain" }
    });
}

export async function emptyJsonResponse() {
    return Response.json({});
}

export async function arrayJsonResponse() {
    return Response.json([1, 2, 3, 4, 5]);
}
"#,
    )
    .unwrap();

    // 2. Module with various export styles (default, named, class, object)
    fs::write(
        tmp.join("exports_styles.ts"),
        r#"
export default async function defaultAction(x?: number, y?: number) {
    return { sum: (x ?? 0) + (y ?? 0) };
}

export async function multiply(x?: number, y?: number) {
    return { product: (x ?? 1) * (y ?? 1) };
}

export class OrderService {
    async execute(orderId: string, amount: number) {
        return { orderId, amount, status: "confirmed" };
    }
}

export const helperObject = {
    async getStatus(service: string) {
        return { service, online: true };
    }
};

export const constantValue = { version: "1.0.0", active: true };
"#,
    )
    .unwrap();

    // 3. Module with argument variations (object vs array vs primitive)
    fs::write(
        tmp.join("args_handling.ts"),
        r#"
export async function objectArg(payload: any) {
    return { received: payload, type: typeof payload };
}

export async function multipleArgs(a: string, b: number, c: boolean) {
    return { a, b, c };
}

export async function noArgs() {
    return { status: "ok" };
}

export async function echoArgs(...all: any[]) {
    return { args: all, count: all.length };
}
"#,
    )
    .unwrap();

    // 4. Module with console.log separation and error handling
    fs::write(
        tmp.join("logs_and_errors.ts"),
        r#"
export async function withLogs() {
    console.log("User debug log 1");
    console.info("User debug log 2");
    return { logged: true };
}

export async function throwStandardError() {
    throw new Error("Explicit business failure");
}

export async function throwCustomString() {
    throw "Custom error string";
}
"#,
    )
    .unwrap();

    // Execute 60 sub-test cases across the modules
    let e2e_cases: Vec<(RpcPayload, bool, serde_json::Value)> = vec![
        // 81-90: Response unwrapping
        (
            RpcPayload { module: "responses".into(), action: Some("jsonResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "success": true, "count": 42, "tags": ["rust", "bun"] })
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("customResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "message": "hello world", "code": 200 })
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("textResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!("plain_text_data")
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("emptyJsonResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({})
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("arrayJsonResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!([1, 2, 3, 4, 5])
        ),
        (
            RpcPayload { module: "./responses.ts".into(), action: Some("jsonResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "success": true, "count": 42, "tags": ["rust", "bun"] })
        ),
        (
            RpcPayload { module: "@/responses".into(), action: Some("customResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "message": "hello world", "code": 200 })
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("arrayJsonResponse".into()), args: Some(serde_json::json!([])), cookies: None },
            true,
            serde_json::json!([1, 2, 3, 4, 5])
        ),
        (
            RpcPayload { module: "responses".into(), action: Some("jsonResponse".into()), args: Some(serde_json::json!(null)), cookies: None },
            true,
            serde_json::json!({ "success": true, "count": 42, "tags": ["rust", "bun"] })
        ),
        (
            RpcPayload { module: "responses.ts".into(), action: Some("emptyJsonResponse".into()), args: None, cookies: None },
            true,
            serde_json::json!({})
        ),

        // 91-105: Export styles (Default, Named, Class with execute, Objects)
        (
            RpcPayload { module: "exports_styles".into(), action: None, args: Some(serde_json::json!([10, 20])), cookies: None },
            true,
            serde_json::json!({ "sum": 30 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("default".into()), args: Some(serde_json::json!([5, 7])), cookies: None },
            true,
            serde_json::json!({ "sum": 12 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("multiply".into()), args: Some(serde_json::json!([6, 7])), cookies: None },
            true,
            serde_json::json!({ "product": 42 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("OrderService".into()), args: Some(serde_json::json!(["ord_123", 99.9])), cookies: None },
            true,
            serde_json::json!({ "orderId": "ord_123", "amount": 99.9, "status": "confirmed" })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("helperObject".into()), args: None, cookies: None },
            true,
            serde_json::json!({})
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("constantValue".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "version": "1.0.0", "active": true })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: None, args: None, cookies: None },
            true,
            serde_json::json!({ "sum": 0 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("multiply".into()), args: Some(serde_json::json!([3, 3])), cookies: None },
            true,
            serde_json::json!({ "product": 9 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("multiply".into()), args: Some(serde_json::json!([0, 100])), cookies: None },
            true,
            serde_json::json!({ "product": 0 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("OrderService".into()), args: Some(serde_json::json!(["ord_999", 500])), cookies: None },
            true,
            serde_json::json!({ "orderId": "ord_999", "amount": 500, "status": "confirmed" })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("default".into()), args: Some(serde_json::json!([100, 200])), cookies: None },
            true,
            serde_json::json!({ "sum": 300 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: None, args: Some(serde_json::json!([1, 1])), cookies: None },
            true,
            serde_json::json!({ "sum": 2 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("multiply".into()), args: Some(serde_json::json!([10, 10])), cookies: None },
            true,
            serde_json::json!({ "product": 100 })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("OrderService".into()), args: Some(serde_json::json!(["test_order", 0])), cookies: None },
            true,
            serde_json::json!({ "orderId": "test_order", "amount": 0, "status": "confirmed" })
        ),
        (
            RpcPayload { module: "exports_styles".into(), action: Some("constantValue".into()), args: Some(serde_json::json!([])), cookies: None },
            true,
            serde_json::json!({ "version": "1.0.0", "active": true })
        ),

        // 106-125: Args handling (Object vs Array vs Null vs Primitive)
        (
            RpcPayload { module: "args_handling".into(), action: Some("objectArg".into()), args: Some(serde_json::json!({ "userId": "u1", "role": "admin" })), cookies: None },
            true,
            serde_json::json!({ "received": { "userId": "u1", "role": "admin" }, "type": "object" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("objectArg".into()), args: Some(serde_json::json!(["hello"])), cookies: None },
            true,
            serde_json::json!({ "received": "hello", "type": "string" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("multipleArgs".into()), args: Some(serde_json::json!(["NATA", 2026, true])), cookies: None },
            true,
            serde_json::json!({ "a": "NATA", "b": 2026, "c": true })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("noArgs".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "status": "ok" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!([1, "two", 3, false, null])), cookies: None },
            true,
            serde_json::json!({ "args": [1, "two", 3, false, null], "count": 5 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!({ "key": "val" })), cookies: None },
            true,
            serde_json::json!({ "args": [{ "key": "val" }], "count": 1 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!("single_string")), cookies: None },
            true,
            serde_json::json!({ "args": ["single_string"], "count": 1 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!(12345)), cookies: None },
            true,
            serde_json::json!({ "args": [12345], "count": 1 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!(true)), cookies: None },
            true,
            serde_json::json!({ "args": [true], "count": 1 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "args": [], "count": 0 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("multipleArgs".into()), args: Some(serde_json::json!(["alpha", 10, false])), cookies: None },
            true,
            serde_json::json!({ "a": "alpha", "b": 10, "c": false })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("noArgs".into()), args: Some(serde_json::json!([])), cookies: None },
            true,
            serde_json::json!({ "status": "ok" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("objectArg".into()), args: Some(serde_json::json!({ "nested": { "a": 1 } })), cookies: None },
            true,
            serde_json::json!({ "received": { "nested": { "a": 1 } }, "type": "object" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!([{"a": 1}, {"b": 2}])), cookies: None },
            true,
            serde_json::json!({ "args": [{"a": 1}, {"b": 2}], "count": 2 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("multipleArgs".into()), args: Some(serde_json::json!(["beta", 99, true])), cookies: None },
            true,
            serde_json::json!({ "a": "beta", "b": 99, "c": true })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("noArgs".into()), args: Some(serde_json::json!(null)), cookies: None },
            true,
            serde_json::json!({ "status": "ok" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("objectArg".into()), args: Some(serde_json::json!(100)), cookies: None },
            true,
            serde_json::json!({ "received": 100, "type": "number" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("objectArg".into()), args: Some(serde_json::json!(false)), cookies: None },
            true,
            serde_json::json!({ "received": false, "type": "boolean" })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("echoArgs".into()), args: Some(serde_json::json!([[1, 2], [3, 4]])), cookies: None },
            true,
            serde_json::json!({ "args": [[1, 2], [3, 4]], "count": 2 })
        ),
        (
            RpcPayload { module: "args_handling".into(), action: Some("multipleArgs".into()), args: Some(serde_json::json!(["gamma", -5, false])), cookies: None },
            true,
            serde_json::json!({ "a": "gamma", "b": -5, "c": false })
        ),

        // 126-140: Logs & Error Handling
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("withLogs".into()), args: None, cookies: None },
            true,
            serde_json::json!({ "logged": true })
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("withLogs".into()), args: Some(serde_json::json!([])), cookies: None },
            true,
            serde_json::json!({ "logged": true })
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwStandardError".into()), args: None, cookies: None },
            false,
            serde_json::json!("Explicit business failure")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwCustomString".into()), args: None, cookies: None },
            false,
            serde_json::json!("Custom error string")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("missingAction".into()), args: None, cookies: None },
            false,
            serde_json::json!("Action 'missingAction' not found")
        ),
        (
            RpcPayload { module: "non_existent_module".into(), action: None, args: None, cookies: None },
            false,
            serde_json::json!("Backend module not found")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("withLogs".into()), args: Some(serde_json::json!(null)), cookies: None },
            true,
            serde_json::json!({ "logged": true })
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwStandardError".into()), args: Some(serde_json::json!([])), cookies: None },
            false,
            serde_json::json!("Explicit business failure")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwCustomString".into()), args: Some(serde_json::json!([])), cookies: None },
            false,
            serde_json::json!("Custom error string")
        ),
        (
            RpcPayload { module: "invalid_mod_name".into(), action: Some("test".into()), args: None, cookies: None },
            false,
            serde_json::json!("Backend module not found")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("withLogs".into()), args: Some(serde_json::json!({"opt": 1})), cookies: None },
            true,
            serde_json::json!({ "logged": true })
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("anotherMissingAction".into()), args: None, cookies: None },
            false,
            serde_json::json!("Action 'anotherMissingAction' not found")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwStandardError".into()), args: Some(serde_json::json!([1, 2])), cookies: None },
            false,
            serde_json::json!("Explicit business failure")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("throwCustomString".into()), args: Some(serde_json::json!({})), cookies: None },
            false,
            serde_json::json!("Custom error string")
        ),
        (
            RpcPayload { module: "logs_and_errors".into(), action: Some("withLogs".into()), args: Some(serde_json::json!("log_arg")), cookies: None },
            true,
            serde_json::json!({ "logged": true })
        ),
    ];

    assert_eq!(e2e_cases.len(), 60, "Section 3 must contain exactly 60 testcases");

    for (idx, (payload, expected_success, expected_val)) in e2e_cases.into_iter().enumerate() {
        let res = RpcExecutor::execute(&tmp, payload).await;
        if expected_success {
            assert!(
                res.is_ok(),
                "Testcase #{:03} failed: expected success, got {:?}",
                idx + 81,
                res
            );
            assert_eq!(
                res.unwrap(),
                expected_val,
                "Testcase #{:03} value mismatch",
                idx + 81
            );
        } else {
            assert!(
                res.is_err(),
                "Testcase #{:03} failed: expected error, got Ok({:?})",
                idx + 81,
                res
            );
            let err_msg = res.unwrap_err();
            let expected_str = expected_val.as_str().unwrap_or("");
            assert!(
                err_msg.contains(expected_str),
                "Testcase #{:03} error message '{:?}' does not contain expected '{:?}'",
                idx + 81,
                err_msg,
                expected_str
            );
        }
    }

    let _ = fs::remove_dir_all(&tmp);
}

// -----------------------------------------------------------------------------
// SECTION 4: Runtime Environment Generation Tests (20 testcases)
// -----------------------------------------------------------------------------

#[test]
fn test_141_to_160_ensure_runtime_env() {
    let tmp = setup_test_project("runtime_env");

    // 141-150: Test creation of runtime files from fresh state
    RpcExecutor::ensure_runtime_env(&tmp);

    let core_ts = tmp.join(".nata/core.ts");
    assert!(core_ts.exists(), "Testcase #141: .nata/core.ts must exist");

    let headers_ts = tmp.join(".nata/headers.ts");
    assert!(headers_ts.exists(), "Testcase #142: .nata/headers.ts must exist");

    let nm_core_pkg = tmp.join("node_modules/core/package.json");
    assert!(nm_core_pkg.exists(), "Testcase #143: node_modules/core/package.json must exist");

    let nm_core_idx = tmp.join("node_modules/core/index.js");
    assert!(nm_core_idx.exists(), "Testcase #144: node_modules/core/index.js must exist");

    let nm_next_headers = tmp.join("node_modules/next/headers.js");
    assert!(nm_next_headers.exists(), "Testcase #145: node_modules/next/headers.js must exist");

    let tsconfig = tmp.join("tsconfig.json");
    assert!(tsconfig.exists(), "Testcase #146: tsconfig.json must exist");

    let core_content = fs::read_to_string(&core_ts).unwrap();
    assert!(core_content.contains("export class HttpError"), "Testcase #147: core.ts contains HttpError");
    assert!(core_content.contains("export function json"), "Testcase #148: core.ts contains json()");
    assert!(core_content.contains("export const db"), "Testcase #149: core.ts contains db proxy");
    assert!(core_content.contains("createDbInstance"), "Testcase #150: core.ts contains createDbInstance");

    // 151-160: Test tsconfig merging and idempotent re-runs
    let tsconfig_content = fs::read_to_string(&tsconfig).unwrap();
    assert!(tsconfig_content.contains("\"core\""), "Testcase #151: tsconfig contains core path");
    assert!(tsconfig_content.contains("\"next/headers\""), "Testcase #152: tsconfig contains next/headers path");
    assert!(tsconfig_content.contains("\"@/*\""), "Testcase #153: tsconfig contains @/* path");

    // Re-run ensure_runtime_env multiple times (idempotence checks)
    for i in 154..=160 {
        RpcExecutor::ensure_runtime_env(&tmp);
        assert!(core_ts.exists(), "Testcase #{}: idempotence check for core.ts", i);
    }

    let _ = fs::remove_dir_all(&tmp);
}

// -----------------------------------------------------------------------------
// SECTION 5: RPC Payload & Serialization Edge Cases (40 testcases)
// -----------------------------------------------------------------------------

#[test]
fn test_161_to_200_rpc_payload_serde_edge_cases() {
    let payloads = vec![
        (r#"{"module":"auth"}"#, "auth", None, None),
        (r#"{"module":"auth","action":"login"}"#, "auth", Some("login"), None),
        (r#"{"module":"users","action":"get","args":[1]}"#, "users", Some("get"), Some(serde_json::json!([1]))),
        (r#"{"module":"billing","args":{"invoiceId":"inv_1"}}"#, "billing", None, Some(serde_json::json!({"invoiceId":"inv_1"}))),
        (r#"{"module":"@/actions/send","action":"default","args":null}"#, "@/actions/send", Some("default"), None),
        (r#"{"module":"./service","action":"run","args":["a","b"]}"#, "./service", Some("run"), Some(serde_json::json!(["a","b"]))),
        (r#"{"module":"deep/nested/mod","action":"init","args":true}"#, "deep/nested/mod", Some("init"), Some(serde_json::json!(true))),
        (r#"{"module":"test","args":100}"#, "test", None, Some(serde_json::json!(100))),
        (r#"{"module":"test","args":-50.5}"#, "test", None, Some(serde_json::json!(-50.5))),
        (r#"{"module":"test","args":[]}"#, "test", None, Some(serde_json::json!([]))),
        (r#"{"module":"test","args":{}}"#, "test", None, Some(serde_json::json!({}))),
        (r#"{"module":"m1","action":"a1","args":[null, false, "str"]}"#, "m1", Some("a1"), Some(serde_json::json!([null, false, "str"]))),
        (r#"{"module":"m2","action":"a2","args":{"a":{"b":{"c":1}}}}"#, "m2", Some("a2"), Some(serde_json::json!({"a":{"b":{"c":1}}}))),
        (r#"{"module":"m3"}"#, "m3", None, None),
        (r#"{"module":"m4","action":""}"#, "m4", Some(""), None),
        (r#"{"module":"m5","action":"custom"}"#, "m5", Some("custom"), None),
        (r#"{"module":"m6","args":[1,2,3,4,5]}"#, "m6", None, Some(serde_json::json!([1,2,3,4,5]))),
        (r#"{"module":"m7","args":"simple_string"}"#, "m7", None, Some(serde_json::json!("simple_string"))),
        (r#"{"module":"m8","action":"exec","args":[true, false]}"#, "m8", Some("exec"), Some(serde_json::json!([true, false]))),
        (r#"{"module":"m9","args":null}"#, "m9", None, None),
    ];

    assert_eq!(payloads.len(), 20, "20 deserialize testcases");

    for (idx, (json_str, exp_mod, exp_act, exp_args)) in payloads.iter().enumerate() {
        let parsed: Result<RpcPayload, _> = serde_json::from_str(json_str);
        assert!(parsed.is_ok(), "Testcase #{:03} JSON parse failure for {:?}", idx + 161, json_str);
        let p = parsed.unwrap();
        assert_eq!(p.module, *exp_mod, "Testcase #{:03} module mismatch", idx + 161);
        assert_eq!(p.action.as_deref(), *exp_act, "Testcase #{:03} action mismatch", idx + 161);
        if let Some(expected_args) = exp_args {
            assert_eq!(p.args.as_ref(), Some(expected_args), "Testcase #{:03} args mismatch", idx + 161);
        }
    }

    // 181-200: RpcResponse serialization testcases
    let response_cases = vec![
        (RpcResponse { success: true, data: Some(serde_json::json!({"id": 1})), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Not found".into()) }, false),
        (RpcResponse { success: true, data: Some(serde_json::json!([1, 2, 3])), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!("ok")), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!(true)), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!(42)), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!(null)), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Unauthorized".into()) }, false),
        (RpcResponse { success: false, data: None, error: Some("Forbidden".into()) }, false),
        (RpcResponse { success: false, data: None, error: Some("Database Error".into()) }, false),
        (RpcResponse { success: true, data: Some(serde_json::json!({})), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!([])), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Internal Error".into()) }, false),
        (RpcResponse { success: true, data: Some(serde_json::json!({"nested": [1, 2]})), error: None }, true),
        (RpcResponse { success: true, data: Some(serde_json::json!({"status": 200})), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Validation failed".into()) }, false),
        (RpcResponse { success: true, data: Some(serde_json::json!("result_text")), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Timeout".into()) }, false),
        (RpcResponse { success: true, data: Some(serde_json::json!(999999)), error: None }, true),
        (RpcResponse { success: false, data: None, error: Some("Unknown".into()) }, false),
    ];

    assert_eq!(response_cases.len(), 20, "20 response serialization testcases");

    for (idx, (resp, should_be_success)) in response_cases.into_iter().enumerate() {
        let serialized = serde_json::to_string(&resp).unwrap();
        assert_eq!(resp.success, should_be_success, "Testcase #{:03} success flag mismatch", idx + 181);
        if should_be_success {
            assert!(serialized.contains("\"success\":true"), "Testcase #{:03} serialization contains success:true", idx + 181);
        } else {
            assert!(serialized.contains("\"success\":false"), "Testcase #{:03} serialization contains success:false", idx + 181);
            assert!(serialized.contains("\"error\":"), "Testcase #{:03} serialization contains error key", idx + 181);
        }
    }
}

// -----------------------------------------------------------------------------
// SECTION 6: RPC Cookie Synchronization Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_201_to_210_rpc_cookie_sync() {
    let tmp = setup_test_project("cookie_sync_test");
    RpcExecutor::ensure_runtime_env(&tmp);

    let auth_mod = tmp.join("modules").join("auth_cookie_test.ts");
    let _ = fs::create_dir_all(auth_mod.parent().unwrap());
    fs::write(
        &auth_mod,
        r#"
import { cookies } from "next/headers";

export async function login(username: string) {
  const cookieStore = await cookies();
  const incomingUser = cookieStore.get("incoming_user")?.value;

  // Set new auth cookie
  cookieStore.set("token", "secret_jwt_" + username, {
    path: "/",
    maxAge: 86400,
    sameSite: "Strict",
  });

  // Delete old cookie
  cookieStore.delete("old_cookie");

  return {
    success: true,
    user: username,
    readFromIncomingCookie: incomingUser || null,
  };
}
"#,
    ).unwrap();

    let payload = RpcPayload {
        module: "modules/auth_cookie_test".into(),
        action: Some("login".into()),
        args: Some(serde_json::json!(["son_nata"])),
        cookies: Some("incoming_user=son_nata_user; old_cookie=to_delete".into()),
    };

    let result = RpcExecutor::execute_full(&tmp, payload).await;
    assert!(result.is_ok(), "RPC execution must succeed: {:?}", result.err());

    let output = result.unwrap();
    assert_eq!(output.data.get("success"), Some(&serde_json::json!(true)));
    assert_eq!(output.data.get("user"), Some(&serde_json::json!("son_nata")));
    assert_eq!(output.data.get("readFromIncomingCookie"), Some(&serde_json::json!("son_nata_user")));

    // Verify set_cookies
    assert!(!output.set_cookies.is_empty(), "Must have set_cookies in output");
    let token_cookie = output.set_cookies.iter().find(|c| c.name == "token").expect("token cookie must be present");
    assert_eq!(token_cookie.value, "secret_jwt_son_nata");
    assert_eq!(token_cookie.max_age, Some(86400));
    assert_eq!(token_cookie.same_site.as_deref(), Some("Strict"));
    assert_eq!(token_cookie.deleted, Some(false));

    let deleted_cookie = output.set_cookies.iter().find(|c| c.name == "old_cookie").expect("old_cookie must be deleted");
    assert_eq!(deleted_cookie.deleted, Some(true));

    let _ = fs::remove_dir_all(&tmp);
}

