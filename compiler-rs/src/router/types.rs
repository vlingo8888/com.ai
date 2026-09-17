use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// The classification of a Next.js / App Router file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteKind {
    Page,
    Api,
    Layout,
    Template,
    Loading,
    Error,
    NotFound,
}

/// The specific type of an App Router URL segment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentType {
    /// Exact static path segment, e.g. `about`, `blog`, `dashboard`
    Static(String),
    /// Dynamic path segment, e.g. `[id]`, `[userId]`
    Dynamic(String),
    /// Catch-all path segment, e.g. `[...slug]`
    CatchAll(String),
    /// Optional catch-all path segment, e.g. `[[...slug]]`
    OptionalCatchAll(String),
    /// Route Group (omitted from URL route), e.g. `(marketing)`, `(auth)`
    RouteGroup(String),
    /// Parallel Route Slot, e.g. `@modal`, `@analytics`
    ParallelSlot(String),
    /// Intercepting Route, e.g. `(.)photo`, `(..)feed`
    Intercepting(String),
}

/// A parsed segment in an App Router path hierarchy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSegment {
    pub raw: String,
    pub segment_type: SegmentType,
}

impl RouteSegment {
    pub fn new(raw: String, segment_type: SegmentType) -> Self {
        Self { raw, segment_type }
    }

    /// Determines if this segment appears in the final HTTP URL path
    pub fn is_url_segment(&self) -> bool {
        !matches!(
            self.segment_type,
            SegmentType::RouteGroup(_) | SegmentType::ParallelSlot(_)
        )
    }

    /// Computes ranking weight for route precedence
    /// (Static: 100 > Dynamic: 50 > OptionalCatchAll: 20 > CatchAll: 10)
    pub fn score(&self) -> i32 {
        match &self.segment_type {
            SegmentType::Static(_) => 100,
            SegmentType::Dynamic(_) => 50,
            SegmentType::OptionalCatchAll(_) => 20,
            SegmentType::CatchAll(_) => 10,
            SegmentType::RouteGroup(_) => 0,
            SegmentType::ParallelSlot(_) => 0,
            SegmentType::Intercepting(_) => 40,
        }
    }
}

/// Represents an indexable, matchable App Router route entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEntry {
    /// Canonical Next.js style route pattern, e.g. `/users/[id]` or `/docs/[...slug]`
    pub pattern: String,
    /// Compiled regular expression string for URL path matching
    pub regex: String,
    /// Relative path to the page or route handler file, e.g. `app/users/[id]/page.tsx`
    pub page_file: PathBuf,
    /// Hierarchical chain of layouts applying to this route, from root layout down to leaf
    pub layout_files: Vec<PathBuf>,
    /// List of dynamic parameter names extracted in order, e.g. `["id"]`
    pub param_names: Vec<String>,
    /// Whether this is a backend API route (`route.ts`) or a frontend UI page (`page.tsx`)
    pub is_api: bool,
    /// Route kind
    pub kind: RouteKind,
    /// Priority score for deterministic sorting and matching precedence
    pub score: i32,
    /// List of parsed route segments
    pub segments: Vec<RouteSegment>,
}

/// Result of matching a request URL path against the App Router table
#[derive(Debug, Clone)]
pub struct RouteMatch {
    /// The matched route definition
    pub route: RouteEntry,
    /// Extracted dynamic parameters (e.g. `{"id": "123"}`)
    pub params: HashMap<String, String>,
    /// Extracted query string parameters (e.g. `{"utm_source": "google"}`)
    pub query: HashMap<String, String>,
    /// The normalized matched request path
    pub matched_path: String,
}
