//! Probing for an RDAP server hosted at a TLD's registry.
//!
//! For a TLD not present in the IANA bootstrap we try a small, ordered set of
//! candidate base URLs. A candidate is considered a live RDAP server when a
//! domain query for `nic.<tld>` against that base returns an RDAP-formatted
//! response. Because the underlying HTTP wrapper does not error on non-2xx
//! status codes, a 404 carrying an RDAP error object still counts as a hit,
//! while HTML / non-RDAP bodies fail to parse and are rejected.
//!
//! Redirect following is disabled on the probe client (see `main`): a 3xx whose
//! body is not RDAP is therefore rejected, which guarantees the base we record
//! actually serves RDAP directly at `<base>/domain/...`.

use icann_rdap_client::http::Client;
use icann_rdap_client::prelude::{rdap_url_request, QueryType};

/// The candidate RDAP base URLs for a TLD, in probe order.
pub fn candidate_bases(tld: &str) -> [String; 3] {
    [
        format!("https://nic.{tld}"),
        format!("https://rdap.nic.{tld}"),
        format!("https://nic.{tld}/rdap"),
    ]
}

/// Return the first candidate base URL (normalised with a trailing `/`) that
/// answers a `nic.<tld>` domain query with an RDAP-formatted response.
pub async fn find_rdap_base(client: &Client, tld: &str) -> Option<String> {
    let probe_domain = format!("nic.{tld}");
    for candidate in candidate_bases(tld) {
        let Ok(query) = QueryType::domain(&probe_domain) else {
            continue;
        };
        let Ok(url) = query.query_url(&candidate) else {
            continue;
        };
        if rdap_url_request(&url, client).await.is_ok() {
            return Some(normalize_base(&candidate));
        }
    }
    None
}

/// Ensure a base URL ends with a single `/` so the bootstrap server can append
/// `domain/<name>` to it.
pub(crate) fn normalize_base(base: &str) -> String {
    if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn GIVEN_tld_WHEN_candidate_bases_THEN_ordered_urls() {
        // GIVEN
        let tld = "example";

        // WHEN
        let actual = candidate_bases(tld);

        // THEN
        assert_eq!(
            actual,
            [
                "https://nic.example".to_string(),
                "https://rdap.nic.example".to_string(),
                "https://nic.example/rdap".to_string(),
            ]
        );
    }

    #[test]
    fn GIVEN_base_without_slash_WHEN_normalize_THEN_trailing_slash_added() {
        // GIVEN / WHEN / THEN
        assert_eq!(
            normalize_base("https://nic.example"),
            "https://nic.example/"
        );
        assert_eq!(
            normalize_base("https://nic.example/rdap"),
            "https://nic.example/rdap/"
        );
        assert_eq!(
            normalize_base("https://nic.example/"),
            "https://nic.example/"
        );
    }
}
