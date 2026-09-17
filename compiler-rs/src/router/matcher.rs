use regex::Regex;
use std::collections::HashMap;

use super::types::{RouteEntry, RouteMatch, RouteSegment, SegmentType};

pub struct RouteMatcher;

impl RouteMatcher {
    /// Normalizes an incoming request URL path and extracts query parameters
    pub fn normalize_url(raw_url: &str) -> (String, HashMap<String, String>) {
        let trimmed = raw_url.trim();

        // Strip hash fragment
        let without_hash = match trimmed.split_once('#') {
            Some((head, _)) => head,
            None => trimmed,
        };

        // Separate path and query string
        let (path_part, query_part) = match without_hash.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (without_hash, None),
        };

        // Parse query params
        let mut query = HashMap::new();
        if let Some(q) = query_part {
            for pair in q.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let (k, v) = match pair.split_once('=') {
                    Some((k, v)) => (Self::url_decode(k), Self::url_decode(v)),
                    None => (Self::url_decode(pair), String::new()),
                };
                if !k.is_empty() {
                    query.insert(k, v);
                }
            }
        }

        // Decode URL percent-encoding in path
        let decoded_path = Self::url_decode(path_part);

        // Normalize multiple slashes and trailing slashes
        let mut clean = String::new();
        let mut prev_slash = false;

        for ch in decoded_path.chars() {
            if ch == '/' {
                if !prev_slash {
                    clean.push('/');
                    prev_slash = true;
                }
            } else {
                clean.push(ch);
                prev_slash = false;
            }
        }

        if clean.is_empty() {
            clean.push('/');
        } else if clean.len() > 1 && clean.ends_with('/') {
            clean.pop();
        }

        (clean, query)
    }

    /// URL percent decoding with full UTF-8 support
    pub fn url_decode(s: &str) -> String {
        let mut bytes_out = Vec::with_capacity(s.len());
        let mut bytes = s.bytes();

        while let Some(b) = bytes.next() {
            if b == b'%' {
                let h1 = bytes.next();
                let h2 = bytes.next();
                if let (Some(h1), Some(h2)) = (h1, h2) {
                    if let (Some(v1), Some(v2)) = (Self::from_hex_digit(h1), Self::from_hex_digit(h2)) {
                        let decoded_byte = (v1 << 4) | v2;
                        bytes_out.push(decoded_byte);
                        continue;
                    } else {
                        bytes_out.push(b'%');
                        bytes_out.push(h1);
                        bytes_out.push(h2);
                        continue;
                    }
                } else {
                    bytes_out.push(b'%');
                    if let Some(h1) = h1 {
                        bytes_out.push(h1);
                    }
                    continue;
                }
            } else if b == b'+' {
                bytes_out.push(b' ');
            } else {
                bytes_out.push(b);
            }
        }

        String::from_utf8_lossy(&bytes_out).to_string()
    }

    fn from_hex_digit(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    /// Compiles parsed segments into a canonical pattern, regex string, param list, and score
    pub fn compile_segments(segments: &[RouteSegment]) -> (String, String, Vec<String>, i32) {
        let mut pattern = String::new();
        let mut regex_str = String::from("^");
        let mut param_names = Vec::new();
        let mut total_score = 0;

        let url_segments: Vec<&RouteSegment> = segments
            .iter()
            .filter(|s| s.is_url_segment())
            .collect();

        if url_segments.is_empty() {
            pattern.push('/');
            regex_str.push_str("/?$");
            total_score += 1000;
        } else {
            for (idx, seg) in url_segments.iter().enumerate() {
                total_score += seg.score();

                match &seg.segment_type {
                    SegmentType::Static(s) => {
                        pattern.push('/');
                        pattern.push_str(s);
                        regex_str.push('/');
                        regex_str.push_str(&regex::escape(s));
                    }
                    SegmentType::Dynamic(param) => {
                        pattern.push_str("/[");
                        pattern.push_str(param);
                        pattern.push(']');
                        regex_str.push_str("/([^/]+)");
                        param_names.push(param.clone());
                    }
                    SegmentType::CatchAll(param) => {
                        pattern.push_str("/[...");
                        pattern.push_str(param);
                        pattern.push(']');
                        regex_str.push_str("/(.+)");
                        param_names.push(param.clone());
                    }
                    SegmentType::OptionalCatchAll(param) => {
                        pattern.push_str("/[[...");
                        pattern.push_str(param);
                        pattern.push_str("]]");
                        if idx == 0 && url_segments.len() == 1 {
                            regex_str.push_str("(?:/(.+))?");
                        } else {
                            regex_str.push_str("(?:/(.+))?");
                        }
                        param_names.push(param.clone());
                    }
                    _ => {}
                }
            }

            regex_str.push_str("/?$");
        }

        (pattern, regex_str, param_names, total_score)
    }

    /// Matches a URL path against a sorted list of RouteEntry routes
    pub fn match_route<'a>(
        routes: &'a [RouteEntry],
        path_or_url: &str,
    ) -> Option<RouteMatch> {
        let (clean_path, query) = Self::normalize_url(path_or_url);

        for route in routes {
            if let Ok(re) = Regex::new(&route.regex) {
                if let Some(caps) = re.captures(&clean_path) {
                    let mut params = HashMap::new();
                    for (i, name) in route.param_names.iter().enumerate() {
                        if let Some(val) = caps.get(i + 1) {
                            let raw_val = val.as_str();
                            if !raw_val.is_empty() {
                                params.insert(name.clone(), raw_val.to_string());
                            }
                        }
                    }

                    return Some(RouteMatch {
                        route: route.clone(),
                        params,
                        query,
                        matched_path: clean_path,
                    });
                }
            }
        }

        None
    }
}
