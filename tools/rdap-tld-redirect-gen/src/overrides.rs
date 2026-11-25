//! Explicit RDAP server overrides for TLDs that do not follow the
//! `nic.<tld>` / `rdap.nic.<tld>` / `nic.<tld>/rdap` probe pattern.
//!
//! The bundled default lives in `overrides.txt` at the crate root and is compiled
//! in via `include_str!`; a different file can be supplied at runtime with
//! `--overrides-file`.

use std::collections::HashMap;

use crate::normalize_tld;
use crate::probe::normalize_base;

/// Parse an overrides mapping from text.
///
/// Each meaningful line is `TLD<whitespace>URL`. Blank lines and `#` comments
/// (whole-line or trailing) are ignored. TLDs are normalised (lower-case, no
/// trailing dot) and base URLs are normalised to end with `/`.
pub fn parse_overrides(text: &str) -> HashMap<String, String> {
    let mut overrides = HashMap::new();

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let mut fields = line.split_whitespace();
        let (Some(tld), Some(url)) = (fields.next(), fields.next()) else {
            continue;
        };

        let tld = normalize_tld(tld);
        if tld.is_empty() {
            continue;
        }

        overrides.insert(tld, normalize_base(url));
    }

    overrides
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn GIVEN_tab_and_space_separated_lines_WHEN_parse_THEN_map_populated() {
        // GIVEN
        let text = ".io\thttps://rdap.identitydigital.services/rdap\n.de https://rdap.denic.de\n";

        // WHEN
        let map = parse_overrides(text);

        // THEN
        assert_eq!(
            map.get("io").expect("io"),
            "https://rdap.identitydigital.services/rdap/"
        );
        assert_eq!(map.get("de").expect("de"), "https://rdap.denic.de/");
    }

    #[test]
    fn GIVEN_comments_and_blanks_WHEN_parse_THEN_ignored() {
        // GIVEN
        let text =
            "# whole line comment\n\n   \n.sh https://rdap.identitydigital.services # trailing\n";

        // WHEN
        let map = parse_overrides(text);

        // THEN
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("sh").expect("sh"),
            "https://rdap.identitydigital.services/"
        );
    }

    #[test]
    fn GIVEN_line_without_url_WHEN_parse_THEN_skipped() {
        // GIVEN
        let text = ".co\n";

        // WHEN
        let map = parse_overrides(text);

        // THEN
        assert!(map.is_empty());
    }

    #[test]
    fn GIVEN_embedded_default_WHEN_parse_THEN_six_known_tlds() {
        // GIVEN
        let text = crate::DEFAULT_OVERRIDES;

        // WHEN
        let map = parse_overrides(text);

        // THEN
        for tld in ["io", "me", "sh", "co", "ru", "de"] {
            assert!(map.contains_key(tld), "missing override for .{tld}");
        }
        assert_eq!(map.get("co").expect("co"), "https://rdap.registry.co/co/");
        assert_eq!(
            map.get("ru").expect("ru"),
            "https://cctld.ru/tci-ripn-rdap/"
        );
    }
}
