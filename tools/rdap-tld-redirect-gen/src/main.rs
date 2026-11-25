//! `rdap-tld-redirect-gen`
//!
//! One-shot generator that creates RDAP 307 redirect templates for top-level
//! domains that are delegated in the DNS root zone but absent from the IANA RDAP
//! DNS bootstrap registry.
//!
//! For each such TLD it probes a small, ordered set of candidate base URLs
//! (`https://nic.<tld>`, `https://rdap.nic.<tld>`, `https://nic.<tld>/rdap`)
//! with a domain query for `nic.<tld>`. The first candidate that answers with an
//! RDAP-formatted response is recorded, and a `tld_redirect_<tld>.template` file
//! is written into the server data directory so the bootstrap server will 307
//! redirect queries for that TLD to the discovered RDAP base.
//!
//! A configurable exception list of TLDs is never scanned. It is empty by default.
//! TLDs whose RDAP server does not follow the probe pattern can be pinned to an
//! explicit base URL via the bundled `overrides.txt` (or `--overrides-file`).

mod overrides;
mod probe;
mod redirect;
mod zone;

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use futures::stream::{self, StreamExt};
use icann_rdap_client::http::{create_client, Client, ClientConfig};
use icann_rdap_client::iana::iana_request;
use icann_rdap_common::iana::{IanaRegistry, IanaRegistryType};

/// Bundled default override mappings (compiled in so the default works from any
/// working directory).
pub(crate) const DEFAULT_OVERRIDES: &str = include_str!("../overrides.txt");

/// Generate RDAP 307 redirect templates for TLDs missing from the IANA bootstrap.
#[derive(Parser, Debug)]
#[command(name = "rdap-tld-redirect-gen", version, about)]
struct Cli {
    /// Data directory where redirect `.template` files are written.
    #[arg(long, env = "RDAP_SRV_DATA_DIR", default_value = "srv/data")]
    data_dir: PathBuf,

    /// URL of the DNS root zone file.
    #[arg(long, default_value = "https://www.internic.net/domain/root.zone")]
    root_zone_url: String,

    /// TLD to exclude from scanning. Repeatable.
    #[arg(long = "exclude")]
    exclude: Vec<String>,

    /// Optional file listing TLDs to exclude (one per line, `#` comments allowed).
    #[arg(long)]
    exclude_file: Option<PathBuf>,

    /// File mapping TLDs to explicit RDAP base URLs for servers that do not
    /// follow the probe pattern. Defaults to the bundled `overrides.txt`.
    #[arg(long)]
    overrides_file: Option<PathBuf>,

    /// Maximum number of concurrent probes.
    #[arg(long, default_value_t = 16)]
    concurrency: usize,

    /// Per-request timeout in seconds.
    #[arg(long, default_value_t = 10)]
    timeout_secs: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let excluded = build_excluded(&cli.exclude, cli.exclude_file.as_deref())?;

    // Plain HTTP client for fetching the (large, plain-text) root zone file.
    let http = reqwest::Client::builder()
        .user_agent("rdap-tld-redirect-gen")
        .timeout(Duration::from_secs(cli.timeout_secs))
        .build()?;

    // RDAP client for probing candidate servers. Redirects are not followed so a
    // 3xx with a non-RDAP body is rejected, guaranteeing the recorded base serves
    // RDAP directly at `<base>/domain/...`.
    let rdap = create_client(
        &ClientConfig::builder()
            .user_agent_suffix("rdap-tld-redirect-gen")
            .timeout_secs(cli.timeout_secs)
            .max_retries(0)
            .follow_redirects(false)
            .build(),
    )?;

    let covered = fetch_covered_tlds(&rdap).await?;
    eprintln!("IANA DNS bootstrap covers {} TLD(s)", covered.len());

    let all_tlds = fetch_root_tlds(&http, &cli.root_zone_url).await?;
    eprintln!("Root zone contains {} TLD(s)", all_tlds.len());

    let uncovered: Vec<String> = all_tlds
        .into_iter()
        .filter(|t| !covered.contains(t) && !excluded.contains(t))
        .collect();
    eprintln!(
        "{} TLD(s) to scan (not in bootstrap, not excluded)",
        uncovered.len()
    );

    fs::create_dir_all(&cli.data_dir)?;
    let removed = redirect::clear_redirect_files(&cli.data_dir)?;
    eprintln!("Cleared {removed} previous redirect file(s)");

    let overrides = match cli.overrides_file.as_deref() {
        Some(path) => overrides::parse_overrides(&fs::read_to_string(path)?),
        None => overrides::parse_overrides(DEFAULT_OVERRIDES),
    };
    let override_hits = uncovered
        .iter()
        .filter(|t| overrides.contains_key(*t))
        .count();
    eprintln!(
        "Loaded {} override mapping(s); {} of the scanned TLD(s) are overridden",
        overrides.len(),
        override_hits
    );

    let rdap = &rdap;
    let overrides = &overrides;
    let mut found: Vec<(String, String)> = stream::iter(uncovered)
        .map(|tld| async move {
            if let Some(base) = overrides.get(&tld) {
                (tld, Some(base.clone()))
            } else {
                let base = probe::find_rdap_base(rdap, &tld).await;
                (tld, base)
            }
        })
        .buffer_unordered(cli.concurrency)
        .filter_map(|(tld, base)| async move { base.map(|b| (tld, b)) })
        .collect()
        .await;
    found.sort();

    let mut written = 0usize;
    for (tld, base) in &found {
        redirect::write_redirect_file(&cli.data_dir, tld, base)?;
        eprintln!("  redirect {tld} -> {base}");
        written += 1;
    }

    eprintln!(
        "Wrote {written} redirect file(s) to {}",
        cli.data_dir.display()
    );
    Ok(())
}

/// Fetch the IANA RDAP DNS bootstrap and collect the set of covered TLDs.
async fn fetch_covered_tlds(client: &Client) -> Result<HashSet<String>, Box<dyn Error>> {
    let response = iana_request(IanaRegistryType::RdapBootstrapDns, client).await?;
    let IanaRegistry::RdapBootstrapRegistry(registry) = response.registry;

    let mut covered = HashSet::new();
    for service in &registry.services {
        if let Some(tlds) = service.first() {
            for tld in tlds {
                let tld = normalize_tld(tld);
                if !tld.is_empty() {
                    covered.insert(tld);
                }
            }
        }
    }
    Ok(covered)
}

/// Fetch the root zone file and return its TLDs.
async fn fetch_root_tlds(http: &reqwest::Client, url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let response = http.get(url).send().await?.error_for_status()?;
    let text = response.text().await?;
    Ok(zone::parse_root_zone(&text))
}

/// Build the exception set from the CLI list and an optional file.
fn build_excluded(cli: &[String], file: Option<&Path>) -> Result<HashSet<String>, Box<dyn Error>> {
    let mut excluded = HashSet::new();

    for entry in cli {
        let tld = normalize_tld(entry);
        if !tld.is_empty() {
            excluded.insert(tld);
        }
    }

    if let Some(path) = file {
        let content = fs::read_to_string(path)?;
        for line in content.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let tld = normalize_tld(line);
            if !tld.is_empty() {
                excluded.insert(tld);
            }
        }
    }

    Ok(excluded)
}

/// Normalise a TLD for comparison: trim whitespace, drop any leading/trailing
/// dots, lower-case. (A TLD never contains interior dots, so trimming both ends
/// is safe and lets callers write either `io` or `.io`.)
pub(crate) fn normalize_tld(s: &str) -> String {
    s.trim().trim_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use icann_rdap_common::response::RdapResponse;
    use icann_rdap_srv::config::{CommonConfig, ServiceConfig};
    use icann_rdap_srv::storage::data::load_data;
    use icann_rdap_srv::storage::mem::{config::MemConfig, ops::Mem};
    use icann_rdap_srv::storage::StoreOps;

    #[test]
    fn GIVEN_mixed_tlds_WHEN_normalize_THEN_lowercased_and_dot_stripped() {
        // GIVEN / WHEN / THEN
        assert_eq!(normalize_tld(" COM. "), "com");
        assert_eq!(normalize_tld("XN--P1AI"), "xn--p1ai");
        assert_eq!(normalize_tld("  "), "");
    }

    #[test]
    fn GIVEN_cli_and_file_WHEN_build_excluded_THEN_merged_set() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("exclude.txt");
        fs::write(&file, "# comment\nfoo.\n  BAR  \n\n").expect("write");

        // WHEN
        let excluded = build_excluded(&["baz".to_string()], Some(&file)).expect("build excluded");

        // THEN
        let mut expected = HashSet::new();
        expected.insert("baz".to_string());
        expected.insert("foo".to_string());
        expected.insert("bar".to_string());
        assert_eq!(excluded, expected);
    }

    #[test]
    fn GIVEN_no_excludes_WHEN_build_excluded_THEN_empty() {
        // GIVEN / WHEN
        let excluded = build_excluded(&[], None).expect("build excluded");

        // THEN
        assert!(excluded.is_empty());
    }

    #[tokio::test]
    async fn GIVEN_generated_redirect_WHEN_loaded_THEN_served_as_307_to_base() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let base = "https://nic.example/rdap/";
        redirect::write_redirect_file(dir.path(), "example", base).expect("write");

        // WHEN
        let mem = Mem::new(
            MemConfig::builder()
                .common_config(CommonConfig::default())
                .build(),
        );
        mem.init().await.expect("init mem");
        let cfg = ServiceConfig::non_server()
            .data_dir(dir.path().to_string_lossy().to_string())
            .build()
            .expect("service config");
        load_data(&cfg, &mem, false).await.expect("load data");

        // THEN
        let resp = mem.get_domain_by_ldh("example").await.expect("lookup");
        assert!(resp.is_redirect(), "expected a redirect, got {resp:?}");
        let RdapResponse::ErrorResponse(err) = resp else {
            panic!("expected an error response");
        };
        assert_eq!(err.error_code(), 307);
        let href = err.common.notices.expect("notices")[0]
            .links
            .as_ref()
            .expect("links")[0]
            .href
            .clone()
            .expect("href");
        assert_eq!(href, base);
    }
}
