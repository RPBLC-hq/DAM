use super::*;

#[test]
fn bundle_assets_are_embedded() {
    // Sanity: the include_str! values exist (placeholder content is fine).
    assert!(!BUNDLE_HTML.is_empty());
    assert!(!FAVICON_SVG.is_empty());
}
