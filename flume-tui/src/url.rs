use std::sync::OnceLock;

use regex::Regex;

static URL_REGEX: OnceLock<Regex> = OnceLock::new();

fn url_regex() -> &'static Regex {
    URL_REGEX.get_or_init(|| {
        Regex::new(r#"https?://[^\s\x00-\x1f<>"'\)\]]+"#).unwrap()
    })
}

/// Return (start, end) byte offsets of all URLs in the text.
pub fn find_urls(text: &str) -> Vec<(usize, usize)> {
    url_regex()
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect()
}

/// Extract URL strings from text.
pub fn extract_urls(text: &str) -> Vec<String> {
    url_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_urls_basic() {
        let text = "check out https://example.com for more info";
        let urls = find_urls(text);
        assert_eq!(urls.len(), 1);
        assert_eq!(&text[urls[0].0..urls[0].1], "https://example.com");
    }

    #[test]
    fn test_find_urls_multiple() {
        let text = "see http://foo.bar and https://baz.qux/path?q=1";
        let urls = find_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(&text[urls[0].0..urls[0].1], "http://foo.bar");
        assert_eq!(&text[urls[1].0..urls[1].1], "https://baz.qux/path?q=1");
    }

    #[test]
    fn test_find_urls_none() {
        assert!(find_urls("no links here").is_empty());
    }

    #[test]
    fn test_extract_urls() {
        let text = "visit https://example.com/page#anchor now";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com/page#anchor"]);
    }

    #[test]
    fn test_url_in_angle_brackets() {
        let text = "link: <https://example.com> here";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com"]);
    }

    #[test]
    fn test_url_in_parens() {
        let text = "(https://example.com)";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com"]);
    }
}
