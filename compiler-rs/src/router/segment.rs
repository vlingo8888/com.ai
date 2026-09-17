use super::types::{RouteSegment, SegmentType};

/// Segment Parser for Next.js App Router conventions
pub struct SegmentParser;

impl SegmentParser {
    /// Parse a directory or filename segment into a strongly typed `RouteSegment`
    pub fn parse(raw: &str) -> RouteSegment {
        let trimmed = raw.trim();

        // 1. Intercepting Routes: (.)photo, (..)feed, (...)root
        if trimmed.starts_with("(.)")
            || trimmed.starts_with("(..)")
            || trimmed.starts_with("(...)")
            || trimmed.starts_with("(..)(..)")
        {
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::Intercepting(trimmed.to_string()),
            );
        }

        // 2. Route Groups: (marketing), (auth), (dashboard)
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = &trimmed[1..trimmed.len() - 1];
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::RouteGroup(inner.to_string()),
            );
        }

        // 2. Parallel Slots: @modal, @sidebar, @analytics
        if trimmed.starts_with('@') {
            let inner = &trimmed[1..];
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::ParallelSlot(inner.to_string()),
            );
        }

        // 3. Optional Catch-All: [[...slug]], [[...categories]]
        if trimmed.starts_with("[[...") && trimmed.ends_with("]]") {
            let inner = &trimmed[5..trimmed.len() - 2];
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::OptionalCatchAll(inner.to_string()),
            );
        }

        // 4. Catch-All: [...slug], [...rest]
        if trimmed.starts_with("[...") && trimmed.ends_with(']') {
            let inner = &trimmed[4..trimmed.len() - 1];
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::CatchAll(inner.to_string()),
            );
        }

        // 5. Dynamic Segment: [id], [userId], [slug]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            return RouteSegment::new(
                trimmed.to_string(),
                SegmentType::Dynamic(inner.to_string()),
            );
        }

        // 6. Static Segment: about, blog, contact, etc.
        RouteSegment::new(
            trimmed.to_string(),
            SegmentType::Static(trimmed.to_string()),
        )
    }

    /// Parse a slice of directory path segments into a list of RouteSegments
    pub fn parse_segments(segments: &[&str]) -> Vec<RouteSegment> {
        segments
            .iter()
            .filter(|s| !s.is_empty() && **s != ".")
            .map(|s| Self::parse(s))
            .collect()
    }
}
