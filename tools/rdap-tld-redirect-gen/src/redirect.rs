//! Building and writing the RDAP redirect template files.
//!
//! A redirect file is a `.template` file in the server data directory holding a
//! `Template::Domain` whose body is a 307 `Rfc9083Error` with a `related` link
//! to the target RDAP base URL, keyed by the TLD's `DomainId`. This is the same
//! shape the server's own IANA bootstrap writer produces, so a query for any
//! name under the TLD is answered with a 307 to `<base>/domain/<name>`.
//!
//! The `tld_redirect_` file-name prefix is deliberately distinct from the
//! `bootstrap_*` prefixes the server removes on each bootstrap cycle, so these
//! files survive bootstrap reloads.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use icann_rdap_common::response::Rfc9083Error;
use icann_rdap_srv::storage::data::{DomainId, DomainOrError, Template};

/// File-name prefix for generated redirect templates.
pub const REDIRECT_PREFIX: &str = "tld_redirect_";

/// Build the redirect template for `tld` pointing at `base`.
pub fn build_redirect_template(base: &str, tld: &str) -> Template {
    let error = Rfc9083Error::redirect().url(base.to_string()).build();
    Template::Domain {
        domain: DomainOrError::ErrorResponse(error),
        ids: vec![DomainId::builder().ldh_name(tld.to_string()).build()],
    }
}

/// Write a redirect template for `tld` -> `base` into `data_dir`.
///
/// Returns the path written.
pub fn write_redirect_file(data_dir: &Path, tld: &str, base: &str) -> io::Result<PathBuf> {
    let template = build_redirect_template(base, tld);
    let path = data_dir.join(format!("{REDIRECT_PREFIX}{tld}.template"));
    let content = serde_json::to_string_pretty(&template)?;
    fs::write(&path, content)?;
    Ok(path)
}

/// Remove previously generated redirect templates from `data_dir`.
///
/// Returns the number of files removed. This makes each run idempotent: TLDs that
/// no longer qualify (e.g. newly added to the exception list) have their stale
/// redirect files cleared.
pub fn clear_redirect_files(data_dir: &Path) -> io::Result<usize> {
    let mut removed = 0;
    if data_dir.is_dir() {
        for entry in fs::read_dir(data_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(REDIRECT_PREFIX) && name.ends_with(".template") {
                fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn GIVEN_base_and_tld_WHEN_build_template_THEN_307_with_link() {
        // GIVEN
        let base = "https://nic.example/rdap/";
        let tld = "example";

        // WHEN
        let template = build_redirect_template(base, tld);

        // THEN
        let json = serde_json::to_value(&template).expect("serialize");
        assert_eq!(json["domain"]["error"]["errorCode"], 307);
        assert_eq!(json["ids"][0]["ldhName"], "example");
        let href = json["domain"]["error"]["notices"][0]["links"][0]["href"]
            .as_str()
            .expect("no href");
        assert_eq!(href, base);
    }

    #[test]
    fn GIVEN_data_dir_WHEN_write_THEN_file_named_with_prefix() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN
        let path =
            write_redirect_file(dir.path(), "example", "https://nic.example/rdap/").expect("write");

        // THEN
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "tld_redirect_example.template"
        );
        assert!(path.exists());
    }

    #[test]
    fn GIVEN_existing_redirects_WHEN_clear_THEN_only_our_prefix_removed() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        write_redirect_file(dir.path(), "foo", "https://nic.foo/").expect("write foo");
        write_redirect_file(dir.path(), "bar", "https://nic.bar/").expect("write bar");
        // an unrelated file that must survive
        fs::write(dir.path().join("bootstrap_dns_0.template"), "{}").expect("write bootstrap");

        // WHEN
        let removed = clear_redirect_files(dir.path()).expect("clear");

        // THEN
        assert_eq!(removed, 2);
        assert!(dir.path().join("bootstrap_dns_0.template").exists());
        assert!(!dir.path().join("tld_redirect_foo.template").exists());
    }

    #[test]
    fn GIVEN_missing_dir_WHEN_clear_THEN_zero_removed() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope");

        // WHEN
        let removed = clear_redirect_files(&missing).expect("clear missing");

        // THEN
        assert_eq!(removed, 0);
    }
}
