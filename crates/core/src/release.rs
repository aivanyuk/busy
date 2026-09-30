//! The opt-in update check's answer (`OptIn::update_check`): whether GitHub's latest release of busy is newer
//! than this build. Pure; the request itself is the app's.

use serde::Deserialize;

/// Where the latest release is asked for: `https://` + `LATEST_HOST` + `LATEST_PATH`.
pub const LATEST_HOST: &str = "api.github.com";
pub const LATEST_PATH: &str = "/repos/aivanyuk/busy/releases/latest";
/// Every release's page starts with this; a link elsewhere is never offered.
pub const RELEASES_URL: &str = "https://github.com/aivanyuk/busy/releases";

/// A release newer than this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// "X.Y.Z", without the tag's `v`.
    pub version: String,
    /// The release's page on GitHub.
    pub url: String,
}

/// The fields read from GitHub's answer; everything else in it is ignored.
#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// "X.Y.Z" as numbers; anything else (a pre-release or build suffix, a missing part) is not a version.
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v
        .split('.')
        .map(|p| if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) { None } else { p.parse().ok() });
    let v = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(v)
}

/// The release in `body` (GitHub's `releases/latest` JSON) if it is newer than `current`: a published,
/// non-pre-release `vX.Y.Z` tag above it, whose page is under [`RELEASES_URL`]. None for anything else,
/// including a body that doesn't parse.
pub fn newer(current: &str, body: &[u8]) -> Option<Release> {
    let l: Latest = serde_json::from_slice(body).ok()?;
    let version = l.tag_name.strip_prefix('v')?;
    let page = format!("{RELEASES_URL}/");
    let newer = parse(version)? > parse(current)?;
    (newer && !l.draft && !l.prerelease && l.html_url.starts_with(&page))
        .then(|| Release { version: version.into(), url: l.html_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(tag: &str, url: &str, extra: &str) -> Vec<u8> {
        format!(r#"{{"tag_name": "{tag}", "html_url": "{url}", "name": "busy", "assets": []{extra}}}"#).into_bytes()
    }

    const PAGE: &str = "https://github.com/aivanyuk/busy/releases/tag/v0.2.0";

    #[test]
    fn a_higher_version_is_newer() {
        let r = newer("0.1.0", &body("v0.2.0", PAGE, "")).unwrap();
        assert_eq!(r, Release { version: "0.2.0".into(), url: PAGE.into() });
        assert!(newer("0.1.9", &body("v0.1.10", PAGE, "")).is_some());
        assert!(newer("0.9.0", &body("v1.0.0", PAGE, "")).is_some());
    }

    #[test]
    fn the_same_or_an_older_version_is_not() {
        assert_eq!(newer("0.2.0", &body("v0.2.0", PAGE, "")), None);
        assert_eq!(newer("0.10.0", &body("v0.9.0", PAGE, "")), None);
    }

    #[test]
    fn drafts_pre_releases_and_odd_tags_are_never_offered() {
        assert_eq!(newer("0.1.0", &body("v0.2.0", PAGE, r#", "draft": true"#)), None);
        assert_eq!(newer("0.1.0", &body("v0.2.0", PAGE, r#", "prerelease": true"#)), None);
        for tag in ["0.2.0", "v0.2.0-rc.1", "v0.2", "v0.2.0.1", "v+1.0.0", "v1..0", "vx.y.z"] {
            assert_eq!(newer("0.1.0", &body(tag, PAGE, "")), None, "{tag}");
        }
    }

    #[test]
    fn only_a_page_under_the_releases_is_linked() {
        for url in
            ["https://example.com/busy/releases/tag/v0.2.0", "http://github.com/aivanyuk/busy/releases/tag/v0.2.0"]
        {
            assert_eq!(newer("0.1.0", &body("v0.2.0", url, "")), None, "{url}");
        }
        assert_eq!(newer("0.1.0", &body("v0.2.0", "https://github.com/aivanyuk/busy/releasesX", "")), None);
    }

    #[test]
    fn a_body_that_does_not_parse_is_nothing() {
        for b in [&b""[..], b"{}", b"not json", br#"{"message": "Not Found"}"#] {
            assert_eq!(newer("0.1.0", b), None);
        }
        assert_eq!(newer("dev", &body("v0.2.0", PAGE, "")), None);
    }
}
