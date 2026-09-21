use super::*;
use std::collections::HashMap;

#[tokio::test]
async fn test_route_handler_get_hello() {
    let temp_dir = std::env::temp_dir().join(format!("nata_rh_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let root = &temp_dir;
    let _ = std::fs::create_dir_all(root);

    let api_dir = root.join("app").join("api").join("hello");
    std::fs::create_dir_all(&api_dir).expect("Failed to create api dir");

    let route_ts = r#"
import { NextResponse } from "next/server";

export async function GET(request: any) {
    return NextResponse.json({ message: "Hello from Com.AI.VN API!", path: request.nextUrl.pathname });
}
"#;
    std::fs::write(api_dir.join("route.ts"), route_ts).expect("Failed to write route.ts");

    let req = RouteHandlerRequest {
        path: "/api/hello".to_string(),
        url: "http://localhost:3000/api/hello".to_string(),
        method: "GET".to_string(),
        file_path: std::path::PathBuf::from("app/api/hello/route.ts"),
        params: HashMap::new(),
        search_params: HashMap::new(),
        headers: HashMap::new(),
        cookies: None,
        body_base64: None,
    };

    let res = RouteHandlerEngine::execute(root, req).await;
    assert_eq!(res.status, 200);

    let body_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &res.body_base64)
        .expect("Failed to decode base64 body");
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse json");
    assert_eq!(body_json["message"], "Hello from Com.AI.VN API!");
    assert_eq!(body_json["path"], "/api/hello");
}

#[tokio::test]
async fn test_route_handler_dynamic_params_and_post_body() {
    let temp_dir = std::env::temp_dir().join(format!("nata_rh_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let root = &temp_dir;
    let _ = std::fs::create_dir_all(root);

    let user_api_dir = root.join("app").join("api").join("users").join("[id]");
    std::fs::create_dir_all(&user_api_dir).expect("Failed to create user api dir");

    let route_ts = r#"
import { NextResponse } from "next/server";

export async function GET(request: any, { params }: any) {
    const id = params.id;
    return NextResponse.json({ userId: id });
}

export async function POST(request: any, { params }: any) {
    const body = await request.json();
    return NextResponse.json({ createdId: params.id, data: body }, { status: 201 });
}
"#;
    std::fs::write(user_api_dir.join("route.ts"), route_ts).expect("Failed to write route.ts");

    // 1. Test GET with dynamic param
    let mut params = HashMap::new();
    params.insert("id".to_string(), "u_42".to_string());

    let get_req = RouteHandlerRequest {
        path: "/api/users/u_42".to_string(),
        url: "http://localhost:3000/api/users/u_42".to_string(),
        method: "GET".to_string(),
        file_path: std::path::PathBuf::from("app/api/users/[id]/route.ts"),
        params: params.clone(),
        search_params: HashMap::new(),
        headers: HashMap::new(),
        cookies: None,
        body_base64: None,
    };

    let get_res = RouteHandlerEngine::execute(root, get_req).await;
    assert_eq!(get_res.status, 200);

    let get_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &get_res.body_base64).unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_json["userId"], "u_42");

    // 2. Test POST with body
    let post_body = serde_json::json!({ "name": "Antigravity", "role": "admin" }).to_string();
    let post_body_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, post_body.as_bytes());

    let mut post_headers = HashMap::new();
    post_headers.insert("content-type".to_string(), "application/json".to_string());

    let post_req = RouteHandlerRequest {
        path: "/api/users/u_42".to_string(),
        url: "http://localhost:3000/api/users/u_42".to_string(),
        method: "POST".to_string(),
        file_path: std::path::PathBuf::from("app/api/users/[id]/route.ts"),
        params,
        search_params: HashMap::new(),
        headers: post_headers,
        cookies: None,
        body_base64: Some(post_body_b64),
    };

    let post_res = RouteHandlerEngine::execute(root, post_req).await;
    assert_eq!(post_res.status, 201);

    let post_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &post_res.body_base64).unwrap();
    let post_json: serde_json::Value = serde_json::from_slice(&post_bytes).unwrap();
    assert_eq!(post_json["createdId"], "u_42");
    assert_eq!(post_json["data"]["name"], "Antigravity");
}

#[tokio::test]
async fn test_route_handler_method_not_allowed_and_options() {
    let temp_dir = std::env::temp_dir().join(format!("nata_rh_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let root = &temp_dir;
    let _ = std::fs::create_dir_all(root);

    let api_dir = root.join("app").join("api").join("items");
    std::fs::create_dir_all(&api_dir).expect("Failed to create items dir");

    let route_ts = r#"
import { NextResponse } from "next/server";

export async function GET() {
    return NextResponse.json({ ok: true });
}
"#;
    std::fs::write(api_dir.join("route.ts"), route_ts).expect("Failed to write route.ts");

    // 1. DELETE is not defined -> 405 Method Not Allowed
    let delete_req = RouteHandlerRequest {
        path: "/api/items".to_string(),
        url: "http://localhost:3000/api/items".to_string(),
        method: "DELETE".to_string(),
        file_path: std::path::PathBuf::from("app/api/items/route.ts"),
        params: HashMap::new(),
        search_params: HashMap::new(),
        headers: HashMap::new(),
        cookies: None,
        body_base64: None,
    };

    let delete_res = RouteHandlerEngine::execute(root, delete_req).await;
    assert_eq!(delete_res.status, 405);
    let allow = delete_res.headers.get("Allow").or_else(|| delete_res.headers.get("allow"));
    assert!(allow.is_some());
    assert!(allow.unwrap().contains("GET"));

    // 2. OPTIONS request automatically returns 204 with Allow header
    let options_req = RouteHandlerRequest {
        path: "/api/items".to_string(),
        url: "http://localhost:3000/api/items".to_string(),
        method: "OPTIONS".to_string(),
        file_path: std::path::PathBuf::from("app/api/items/route.ts"),
        params: HashMap::new(),
        search_params: HashMap::new(),
        headers: HashMap::new(),
        cookies: None,
        body_base64: None,
    };

    let options_res = RouteHandlerEngine::execute(root, options_req).await;
    assert_eq!(options_res.status, 204);
    let allow_options = options_res.headers.get("Allow").or_else(|| options_res.headers.get("allow"));
    assert!(allow_options.is_some());
    assert!(allow_options.unwrap().contains("GET"));
}
