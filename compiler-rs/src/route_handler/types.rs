use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// Input request context dispatched to a Next.js route handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteHandlerRequest {
    /// Request URL pathname, e.g. `/api/users/123`
    pub path: String,
    /// Full URL string including origin and query parameters, e.g. `http://localhost:3000/api/users/123?role=admin`
    pub url: String,
    /// HTTP method, e.g. `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `OPTIONS`, `HEAD`
    pub method: String,
    /// Relative or absolute path to the route file, e.g. `app/api/users/[id]/route.ts`
    pub file_path: PathBuf,
    /// Dynamic parameters extracted by router, e.g. `{"id": "123"}`
    pub params: HashMap<String, String>,
    /// Parsed query string parameters, e.g. `{"role": "admin"}`
    pub search_params: HashMap<String, String>,
    /// Request headers received from client
    pub headers: HashMap<String, String>,
    /// Raw `Cookie` header string
    pub cookies: Option<String>,
    /// Request body encoded as base64 string, or empty if no body
    pub body_base64: Option<String>,
}

/// A cookie directive to be set on the response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCookie {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "maxAge")]
    pub max_age: Option<i64>,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub secure: Option<bool>,
    #[serde(default, rename = "sameSite")]
    pub same_site: Option<String>,
    #[serde(default)]
    pub deleted: Option<bool>,
}

/// Output returned from executing a Next.js route handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteHandlerResponse {
    /// HTTP status code (e.g. 200, 201, 400, 404, 405, 500)
    pub status: u16,
    /// HTTP status text, e.g. "OK", "Created", "Method Not Allowed"
    pub status_text: String,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Cookies to set via `Set-Cookie` header
    #[serde(default)]
    pub cookies: Vec<RouteCookie>,
    /// Body content encoded in base64
    #[serde(default)]
    pub body_base64: String,
    /// Execution time in milliseconds
    #[serde(default)]
    pub duration_ms: f64,
    /// Error message if an uncaught exception occurred
    pub error: Option<String>,
}

impl Default for RouteHandlerResponse {
    fn default() -> Self {
        Self {
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            cookies: Vec::new(),
            body_base64: String::new(),
            duration_ms: 0.0,
            error: None,
        }
    }
}
