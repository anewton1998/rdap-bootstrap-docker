//! Building and writing the RDAP server help file that lists synthesized redirects.
//!
//! The `rdap-srv` server serves RDAP help at `GET /rdap/help` by reading `.help`
//! files from its data directory. The file name encodes the host it applies to:
//! the loader strips the `.help` suffix and replaces `_` with `.`, so a file
//! named `__default.help` maps to the `..default` key
//! (`icann_rdap_srv::storage::DEFAULT_HELPFILE_NAME`) and is served for any
//! host without a host-specific help file.
//!
//! The file body must be an RDAP document that `RdapResponse::try_from` reads
//! back as a `Help` response: that means a JSON object with a `notices` array
//! and **no** `objectClassName`. We build it with the `icann-rdap-common`
//! builders so it is guaranteed to round-trip.
//!
//! This tool owns `__default.help` and overwrites it on every run (unless
//! `--no-help-file` is given, in which case a stale copy is removed).

use std::io;
use std::path::{Path, PathBuf};

use icann_rdap_common::prelude::{Help, Notice, RdapResponse, ToResponse};

/// File name of the generated default help file.
///
/// Maps to the `..default` help key when loaded by the server.
pub const HELP_FILE_NAME: &str = "__default.help";

/// How a redirect entry was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// Pinned via the overrides file.
    Override,
    /// Discovered by probing candidate RDAP base URLs.
    Probed,
}

impl Source {
    /// Short label used in the help listing.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Override => "override",
            Self::Probed => "probe",
        }
    }
}

/// A single synthesized redirect: TLD, target base URL, and how it was found.
pub type Entry = (String, String, Source);

/// Build the RDAP help response listing every synthesized redirect.
///
/// Produces a single `Notice` titled "Synthesized RDAP Redirects" whose
/// description is a summary line followed by one line per entry. The document
/// intentionally has no `objectClassName` so `RdapResponse::try_from` reads it
/// back as a `Help` response.
pub fn build_help_response(entries: &[Entry]) -> RdapResponse {
    let (probed, overridden) = entries
        .iter()
        .fold((0usize, 0usize), |(p, o), e| match e.2 {
            Source::Probed => (p + 1, o),
            Source::Override => (p, o + 1),
        });

    let mut description = Vec::with_capacity(entries.len() + 1);
    description.push(format!(
        "{n} TLD(s) redirected: {probed} discovered by probe, {overridden} from overrides.",
        n = entries.len()
    ));
    for (tld, base, source) in entries {
        description.push(format!(".{tld} -> {base} ({})", source.label()));
    }

    let notice = Notice::builder()
        .title("Synthesized RDAP Redirects")
        .description(description)
        .build();

    Help::response().notices(vec![notice]).build().to_response()
}

/// Write the help response to `__default.help` in `data_dir`.
///
/// Returns the path written.
pub fn write_help_file(data_dir: &Path, response: &RdapResponse) -> io::Result<PathBuf> {
    let path = data_dir.join(HELP_FILE_NAME);
    let mut content = serde_json::to_string_pretty(response)?;
    content.push('\n');
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Remove the generated help file (`__default.help`) from `data_dir`.
///
/// Returns the number of files removed (0 or 1). Used when `--no-help-file` is
/// set so a previously generated listing does not go stale.
pub fn clear_help_file(data_dir: &Path) -> io::Result<usize> {
    let path = data_dir.join(HELP_FILE_NAME);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(1),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use icann_rdap_srv::config::{CommonConfig, ServiceConfig};
    use icann_rdap_srv::storage::data::load_data;
    use icann_rdap_srv::storage::mem::{config::MemConfig, ops::Mem};
    use icann_rdap_srv::storage::StoreOps;

    fn sample_entries() -> Vec<Entry> {
        vec![
            (
                "io".to_string(),
                "https://rdap.identitydigital.services/rdap/".to_string(),
                Source::Override,
            ),
            (
                "ch".to_string(),
                "https://rdap.nic.ch/".to_string(),
                Source::Probed,
            ),
        ]
    }

    #[test]
    fn GIVEN_entries_WHEN_build_help_response_THEN_parses_back_as_help_with_lines() {
        // GIVEN
        let entries = sample_entries();

        // WHEN
        let response = build_help_response(&entries);
        let json = serde_json::to_value(&response).expect("serialize");

        // THEN
        // No objectClassName so it round-trips as Help.
        assert!(json.get("objectClassName").is_none());
        let round_tripped = RdapResponse::try_from(json.clone()).expect("try_from");
        assert!(
            matches!(round_tripped, RdapResponse::Help(_)),
            "expected Help, got {round_tripped:?}"
        );

        // The notice description contains the summary and one line per entry.
        let descriptions = json["notices"][0]["description"]
            .as_array()
            .expect("description array");
        let joined = descriptions
            .iter()
            .map(|v| v.as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(descriptions.len(), 3, "summary + 2 entries");
        assert!(joined.contains("2 TLD(s) redirected: 1 discovered by probe, 1 from overrides."));
        assert!(joined.contains(".io -> https://rdap.identitydigital.services/rdap/ (override)"));
        assert!(joined.contains(".ch -> https://rdap.nic.ch/ (probe)"));
    }

    #[test]
    fn GIVEN_empty_entries_WHEN_build_help_response_THEN_summary_only() {
        // GIVEN / WHEN
        let response = build_help_response(&[]);
        let json = serde_json::to_value(&response).expect("serialize");

        // THEN
        let descriptions = json["notices"][0]["description"]
            .as_array()
            .expect("description array");
        assert_eq!(descriptions.len(), 1);
        assert_eq!(
            descriptions[0].as_str().unwrap(),
            "0 TLD(s) redirected: 0 discovered by probe, 0 from overrides."
        );
    }

    #[test]
    fn GIVEN_data_dir_WHEN_write_help_file_THEN_named_default() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let response = build_help_response(&sample_entries());

        // WHEN
        let path = write_help_file(dir.path(), &response).expect("write");

        // THEN
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "__default.help"
        );
        assert!(path.exists());
    }

    #[tokio::test]
    async fn GIVEN_written_help_WHEN_loaded_THEN_served_as_default_help() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let response = build_help_response(&sample_entries());
        write_help_file(dir.path(), &response).expect("write");

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
        let served = mem.get_srv_help(None).await.expect("get help");
        assert!(
            matches!(served, RdapResponse::Help(_)),
            "expected Help, got {served:?}"
        );
        let RdapResponse::Help(help) = served else {
            unreachable!()
        };
        let text = help.common.notices.as_ref().expect("notices")[0]
            .description()
            .join("\n");
        assert!(text.contains(".io -> https://rdap.identitydigital.services/rdap/ (override)"));
        assert!(text.contains(".ch -> https://rdap.nic.ch/ (probe)"));
    }

    #[test]
    fn GIVEN_mixed_files_WHEN_clear_help_file_THEN_only_default_removed() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let response = build_help_response(&sample_entries());
        write_help_file(dir.path(), &response).expect("write help");
        std::fs::write(dir.path().join("tld_redirect_io.template"), "{}").expect("write template");
        std::fs::write(dir.path().join("bootstrap_dns_0.template"), "{}").expect("write bootstrap");

        // WHEN
        let removed = clear_help_file(dir.path()).expect("clear");

        // THEN
        assert_eq!(removed, 1);
        assert!(!dir.path().join("__default.help").exists());
        assert!(dir.path().join("tld_redirect_io.template").exists());
        assert!(dir.path().join("bootstrap_dns_0.template").exists());
    }

    #[test]
    fn GIVEN_no_help_file_WHEN_clear_THEN_zero_removed() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN / THEN
        assert_eq!(clear_help_file(dir.path()).expect("clear"), 0);
    }
}
