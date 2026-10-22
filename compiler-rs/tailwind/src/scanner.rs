//! Candidate scanning for project files.
//!
//! Template files go through the port of Tailwind's oxide extractor
//! ([`crate::scan`]), so the candidate set matches what `@tailwindcss/oxide`
//! (`Scanner::scanFiles`) extracts. CSS files are resolved into the custom CSS
//! compiled together with the base stylesheet ([`crate::css_resolve`]), which
//! also decides which files are scanned (`@source`, `source(none)`).

use crate::css_resolve::{self, Sources};
use crate::scan;
use crate::types::FileItem;
use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::LazyLock;
use xxhash_rust::xxh3::Xxh3;

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub tokens: Vec<String>,
    pub custom_css: String,
    pub signature_hash: String,
}

/// Directories oxide never scans (`scanner/fixtures/ignored-content-dirs.txt`).
const IGNORED_CONTENT_DIRS: &[&str] = &[
    ".git", ".hg", ".jj", ".next", ".parcel-cache", ".pnpm-store", ".svelte-kit", ".svn", ".turbo", ".venv",
    ".vercel", ".yarn", "__pycache__", "node_modules", "venv",
];

/// `scanner/fixtures/ignored-extensions.txt` (CSS is handled separately).
const IGNORED_EXTENSIONS: &[&str] = &["less", "lock", "sass", "scss", "styl", "log"];

/// `scanner/fixtures/ignored-files.txt` (`.env.*` is matched by prefix).
const IGNORED_FILES: &[&str] = &["package-lock.json", "pnpm-lock.yaml", "bun.lockb", ".gitignore", ".env"];

/// `scanner/fixtures/binary-extensions.txt`
static BINARY_EXTENSIONS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| include_str!("scan/fixtures/binary-extensions.txt").lines().map(str::trim).filter(|l| !l.is_empty()).collect());

/// Files oxide never reads (lockfiles, binaries, `.env`…), wherever they are.
pub fn is_scannable_file(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if IGNORED_FILES.contains(&name) || name.starts_with(".env.") {
        return false;
    }
    let ext = scan::extension_of(name);
    !IGNORED_EXTENSIONS.contains(&ext) && !BINARY_EXTENSIONS.contains(ext)
}

/// Oxide's auto source detection rules, applied to a project-relative path.
fn is_template_file(path: &str) -> bool {
    let mut segments = path.split(['/', '\\']).filter(|s| !s.is_empty() && *s != ".");
    if segments.next_back().is_none() {
        return false;
    }
    if segments.any(|dir| IGNORED_CONTENT_DIRS.contains(&dir)) {
        return false;
    }
    is_scannable_file(path)
}

/// Whether a (non-CSS) file is scanned: automatic detection (unless
/// `source(none)`, limited to `source("dir")`), then the `@source` rules in
/// order. An explicit `@source` also reaches ignored folders like
/// `node_modules`, and a glob selects exactly what it matches.
pub fn is_scanned(path: &str, sources: &Sources) -> bool {
    let p = css_resolve::normalize(path);
    let mut scanned = sources.auto && css_resolve::is_under(&p, &sources.auto_base) && is_template_file(&p);
    for rule in &sources.rules {
        if rule.matches(&p) {
            scanned = !rule.negated && (rule.is_glob() || is_scannable_file(&p));
        }
    }
    scanned
}

/// Compute fast 64-bit XXH3 signature of all files (31 GB/s throughput)
pub fn compute_files_signature(files: &[FileItem]) -> String {
    let mut hasher = Xxh3::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(b":");
        let content_hash = xxhash_rust::xxh3::xxh3_64(file.content.as_bytes());
        hasher.update(&content_hash.to_le_bytes());
        hasher.update(b"|");
    }
    format!("{:016x}", hasher.digest())
}

/// Candidates of a single template file (pre-processed by its extension).
pub fn extract_file_candidates(path: &str, content: &str) -> Vec<String> {
    let mut set = HashSet::new();
    scan::extract_candidates(content.as_bytes(), scan::extension_of(path), &mut set);
    let mut tokens: Vec<String> = set.into_iter().collect();
    tokens.sort_unstable();
    tokens
}

pub fn scan_files_parallel(files: &[FileItem]) -> ScanResult {
    let signature_hash = compute_files_signature(files);

    let resolved = css_resolve::resolve_css(files);
    let custom_css = resolved.css;

    // Template files in parallel, largest first so one big file doesn't end up
    // as the tail of the batch.
    let mut templates: Vec<&FileItem> = files
        .iter()
        .filter(|f| !f.path.is_empty() && !f.path.ends_with(".css") && is_scanned(&f.path, &resolved.sources))
        .collect();
    templates.sort_unstable_by_key(|f| std::cmp::Reverse(f.content.len()));

    let sets: Vec<HashSet<String>> = templates
        .par_iter()
        .map(|file| {
            let mut tokens = HashSet::new();
            scan::extract_candidates(file.content.as_bytes(), scan::extension_of(&file.path), &mut tokens);
            tokens
        })
        .collect();

    let mut all_tokens_set = HashSet::new();
    for set in sets {
        all_tokens_set.extend(set);
    }

    let mut tokens: Vec<String> = all_tokens_set.into_iter().collect();
    tokens.sort();

    ScanResult {
        tokens,
        custom_css,
        signature_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_user_html_snippet() {
        let html_content = r##"
<div class="flex flex-col w-[390px] min-h-[844px] bg-[#F4F1EC] text-[#12272B] font-sans p-0 gap-0 overflow-hidden">
  <!-- Thương hiệu & Tiêu đề trang -->
  <div class="flex flex-col items-center text-center pt-10 pb-2 gap-0">
    <span style="font-family:'Cormorant Garamond',serif;font-size:24px;font-weight:300;letter-spacing:8px" class="text-[#12272B] uppercase mb-1">WAIKI</span>
    <span style="font-family:'Inter',sans-serif;font-size:10px;font-weight:500;letter-spacing:3px" class="text-[#8B806B] uppercase">Zen Wellness Resort</span>
    <div class="w-8 h-px bg-[#BBAD91] mt-4 opacity-60"></div>
  </div>
  <div class="flex flex-col w-full max-w-[390px] px-5 py-8">
    <div class="flex flex-col w-full bg-white border border-[#DDD8D0] rounded-[16px] p-6">
      <div class="flex justify-center border-b border-[#DDD8D0] mb-8">
        <button style="font-family:'Inter',sans-serif;font-size:11px;font-weight:500;letter-spacing:2px" class="uppercase pb-3 px-6 border-b-2 text-[#5B2823] border-[#5B2823]">Khách Hàng</button>
        <button style="font-family:'Inter',sans-serif;font-size:11px;font-weight:500;letter-spacing:2px" class="uppercase pb-3 px-6 border-b-2 text-[#8B806B]/60 border-transparent">Ban Vận Hành</button>
      </div>
      <div class="flex flex-col w-full gap-5">
        <div class="flex items-center justify-center -mt-16">
          <div class="w-[68px] h-[68px] rounded-full bg-[#F8F6F1] flex items-center justify-center">
            <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#B59655" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            </svg>
          </div>
        </div>
        <div class="flex flex-col items-center gap-1 mt-2">
          <p class="text-[#315D52] uppercase text-center">Private Access</p>
          <div class="w-[32px] h-[1px] bg-[#D8CCB4] mx-auto mt-2.5"></div>
        </div>
        <div class="flex flex-col w-full gap-1.5">
          <label class="text-[#A9874F] uppercase">Mã Đặt Chỗ (Booking Code)</label>
          <div class="flex items-center w-full px-3 bg-[#FBF9F5] border border-[#E7E1D8] rounded-[10px]">
            <div class="w-7 h-7 rounded-full bg-[#F4F1EA] flex items-center justify-center flex-shrink-0 mr-2.5">
              <i data-lucide="key" class="w-3.5 h-3.5 text-[#B59655]"></i>
            </div>
            <span class="text-[#A8A29A]/50">ZEN-123456</span>
          </div>
          <p class="leading-[1.4] text-[#A8A29A] italic">*Mã xác thực được gửi qua tin nhắn khi hoàn tất đặt chỗ.</p>
        </div>
        <div class="flex items-start gap-2.5 py-1 px-1">
          <div class="flex items-start gap-2.5 w-full">
            <div class="w-[20px] h-[20px] rounded-[6px] bg-[#315D52] border border-[#315D52] flex items-center justify-center flex-shrink-0 mt-0.5 box-border">
              <i data-lucide="check" class="w-4 h-4 text-white"></i>
            </div>
            <span class="text-[#8B806B] leading-[1.5] text-left">
              Tôi đồng ý với các <span class="underline font-medium">Điều khoản</span>
            </span>
          </div>
        </div>
        <button class="w-full flex items-center justify-between px-4 text-white font-medium uppercase">
          <span class="flex-1 text-center pl-6 truncate">Bắt Đầu Hành Trình</span>
          <i data-lucide="arrow-right" class="w-4 h-4 text-white flex-shrink-0"></i>
        </button>
        <div class="flex items-center gap-3 py-2">
          <div class="flex-grow h-px bg-[#E6DED2]"></div>
          <span class="text-[#B59655]">OR</span>
          <div class="flex-grow h-px bg-[#E6DED2]"></div>
        </div>
        <div class="flex flex-col items-center gap-4">
          <div class="inline-flex items-center gap-1.5">
            <div class="w-[22px] h-[22px] rounded-full bg-[#F4F1EA] flex items-center justify-center">
              <i data-lucide="help-circle" class="w-3 h-3 text-[#315D52]"></i>
            </div>
            <span class="underline">Quên mã đặt chỗ?</span>
          </div>
          <div class="w-full flex flex-col items-center gap-2">
            <p class="text-[#A8A29A] text-center">Bạn chưa có trải nghiệm nào?</p>
            <button class="w-full flex items-center justify-center uppercase font-medium bg-transparent">Khám Phá WAIKI</button>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="flex flex-col items-center text-center pb-8">
    <p class="text-[#8B806B]/50">Trải nghiệm chữa lành</p>
    <p class="text-[#8B806B]/50 uppercase mt-1">© 2025 WAIKI Resort</p>
  </div>
</div>
        "##;

        let files = vec![FileItem {
            path: "index.html".to_string(),
            content: html_content.to_string(),
        }];

        let result = scan_files_parallel(&files);
        assert!(result.tokens.contains(&"-mt-16".to_string()));
        assert!(result.tokens.contains(&"text-[#8B806B]/60".to_string()));
        assert!(result.tokens.contains(&"text-[#A8A29A]/50".to_string()));
        assert!(result.tokens.contains(&"leading-[1.4]".to_string()));
        assert!(result.tokens.contains(&"leading-[1.5]".to_string()));
        assert!(result.tokens.contains(&"flex-shrink-0".to_string()));
        assert!(result.tokens.contains(&"flex-grow".to_string()));
        assert!(result.tokens.contains(&"w-[390px]".to_string()));
        assert!(result.tokens.contains(&"min-h-[844px]".to_string()));
        assert!(result.tokens.contains(&"bg-[#F4F1EC]".to_string()));
    }
}
