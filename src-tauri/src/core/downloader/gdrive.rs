//! Google Drive URL resolution
//!
//! Converts various Google Drive URL formats to direct download links.

/// Convert a Google Drive URL to a direct download link.
///
/// Handles multiple URL patterns:
/// - `/file/d/{id}/view`
/// - `/open?id={id}`
/// - `/uc?id={id}`
/// - `/uc?export=download&id={id}`
pub fn resolve_gdrive_url(url: &str, gdrive_id: Option<&str>) -> String {
    if let Some(gid) = gdrive_id {
        format!(
            "https://drive.google.com/uc?export=download&id={}&confirm=t",
            gid
        )
    } else {
        // Try to extract ID from URL
        let patterns = [
            regex::Regex::new(r"drive\.google\.com/file/d/([^/?#]+)").ok(),
            regex::Regex::new(r"[?&]id=([^&]+)").ok(),
        ];

        for pat in patterns.iter().flatten() {
            if let Some(caps) = pat.captures(url) {
                if let Some(m) = caps.get(1) {
                    return format!(
                        "https://drive.google.com/uc?export=download&id={}&confirm=t",
                        m.as_str()
                    );
                }
            }
        }

        // Fallback: return original URL
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_with_id() {
        let url = resolve_gdrive_url("https://drive.google.com/file/d/ABC/view", Some("ABC"));
        assert_eq!(
            url,
            "https://drive.google.com/uc?export=download&id=ABC&confirm=t"
        );
    }

    #[test]
    fn test_resolve_without_id() {
        let url =
            resolve_gdrive_url("https://drive.google.com/file/d/XYZ123/view?usp=sharing", None);
        assert_eq!(
            url,
            "https://drive.google.com/uc?export=download&id=XYZ123&confirm=t"
        );
    }

    #[test]
    fn test_resolve_fallback() {
        let url = resolve_gdrive_url("https://drive.google.com/something-weird", None);
        assert_eq!(url, "https://drive.google.com/something-weird");
    }
}
