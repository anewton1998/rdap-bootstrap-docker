//! Parsing of the DNS root zone file to extract the set of top-level domains.
//!
//! A TLD is identified as a record whose owner is a single label (one name, no
//! interior dots) and whose record type is `NS`. The root owner (`.` / `@`),
//! multi-label owners (second-level names and glue records), non-`NS` records
//! (e.g. `DS`), comments, directives (`$TTL`) and multi-line continuation lines
//! are all ignored.

/// Extract the distinct TLD names (lower-case, no trailing dot) from a root zone file.
pub fn parse_root_zone(text: &str) -> Vec<String> {
    let mut tlds: Vec<String> = Vec::new();

    for raw in text.lines() {
        // Continuation lines of a multi-line record start with whitespace; the
        // owner is inherited, so they never introduce a new TLD.
        if raw.starts_with(char::is_whitespace) {
            continue;
        }

        // Strip any trailing comment and surrounding whitespace.
        let line = raw.split(';').next().unwrap_or("").trim_end();
        if line.is_empty() || line.starts_with('$') {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(owner) = fields.first() else {
            continue;
        };

        // Owner must be fully qualified (trailing dot) and a single label.
        let Some(label) = owner.strip_suffix('.') else {
            continue;
        };
        if label.is_empty() || label.contains('.') {
            continue;
        }

        // Only delegated TLDs have an NS record at the apex.
        if record_type(&fields) != Some("NS") {
            continue;
        }

        tlds.push(label.to_ascii_lowercase());
    }

    tlds.sort();
    tlds.dedup();
    tlds
}

/// Determine the record type of a zone-file line.
///
/// The type is the first field after the owner that is neither a TTL (all digits)
/// nor a DNS class (`IN`/`CH`/`HS`/`CHAOS`). This tolerates both the
/// `owner ttl class type` and `owner class ttl type` orderings.
fn record_type<'a>(fields: &[&'a str]) -> Option<&'a str> {
    for field in fields.iter().skip(1) {
        if !field.is_empty() && field.chars().all(|c| c.is_ascii_digit()) {
            continue; // TTL
        }
        match field.to_ascii_uppercase().as_str() {
            "IN" | "CH" | "HS" | "CHAOS" => continue, // class
            _ => return Some(*field),
        }
    }
    None
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn GIVEN_root_zone_WHEN_parse_THEN_returns_single_label_ns_tlds() {
        // GIVEN
        let zone = "\
$TTL 3600000
@ IN SOA a.root-servers.net. nstld.verisign-grs.com. (
        2024111400 ; serial
        1800       ; refresh
        )
@ IN NS a.root-servers.net.
com. 172800 IN NS a.gtld-servers.net.
net. 172800 IN NS b.gtld-servers.net.
";

        // WHEN
        let actual = parse_root_zone(zone);

        // THEN
        assert_eq!(actual, vec!["com".to_string(), "net".to_string()]);
    }

    #[test]
    fn GIVEN_mixed_records_WHEN_parse_THEN_skips_non_ns_and_multilabel() {
        // GIVEN
        let zone = "\
com. 172800 IN NS a.gtld-servers.net.
com. 3600 IN DS 19718 8 2 ABCDEF
example.com. 3600 IN A 192.0.2.1
a.gtld-servers.net. 3600 IN A 192.5.5.241
xn--p1ai. 172800 IN NS d.nstld.com.
";

        // WHEN
        let actual = parse_root_zone(zone);

        // THEN
        assert_eq!(actual, vec!["com".to_string(), "xn--p1ai".to_string()]);
    }

    #[test]
    fn GIVEN_comments_and_blanks_WHEN_parse_THEN_ignored() {
        // GIVEN
        let zone = "\
; a comment at column zero

   ; indented continuation comment
org. 172800 IN NS a.gtld-servers.net. ; trailing comment
";

        // WHEN
        let actual = parse_root_zone(zone);

        // THEN
        assert_eq!(actual, vec!["org".to_string()]);
    }

    #[test]
    fn GIVEN_duplicate_tld_records_WHEN_parse_THEN_deduplicated() {
        // GIVEN
        let zone = "\
info. 172800 IN NS a.gtld-servers.net.
info. 172800 IN NS b.gtld-servers.net.
";

        // WHEN
        let actual = parse_root_zone(zone);

        // THEN
        assert_eq!(actual, vec!["info".to_string()]);
    }

    #[test]
    fn GIVEN_class_before_ttl_WHEN_parse_THEN_type_still_resolved() {
        // GIVEN
        let zone = "museum. IN 172800 NS a.gtld-servers.net.\n";

        // WHEN
        let actual = parse_root_zone(zone);

        // THEN
        assert_eq!(actual, vec!["museum".to_string()]);
    }
}
