# rdap-tld-redirect-gen

A one-shot generator that creates RDAP **307 redirect** templates for top-level
domains that are delegated in the DNS root zone but **absent from the IANA RDAP
DNS bootstrap registry**.

For each such TLD it probes a small, ordered set of candidate base URLs and, when
an RDAP server is found, writes a `tld_redirect_<tld>.template` file into the
`icann-rdap-srv` data directory. The bootstrap server then answers queries for
any name under that TLD with a `307` redirect to the discovered RDAP base.

## How it works

1. Fetch the IANA RDAP DNS bootstrap (`https://data.iana.org/rdap/dns.json`) and
   collect the set of covered TLDs.
2. Fetch the DNS root zone (`https://www.internic.net/domain/root.zone`) and
   collect every single-label `NS` owner (the delegated TLDs).
3. `uncovered = root-zone TLDs − bootstrap TLDs − excluded TLDs`.
4. For each uncovered TLD, probe these candidates **in order** with a domain query
   for `nic.<tld>`:
   - `https://nic.<tld>`
   - `https://rdap.nic.<tld>`
   - `https://nic.<tld>/rdap`

   The first candidate that answers with an **RDAP-formatted response** wins.
   Because the HTTP layer does not error on non-2xx status, a `404` carrying an
   RDAP error object still counts as a live RDAP server, while HTML / non-RDAP
   bodies are rejected. Redirects are **not** followed, so the recorded base is
   guaranteed to serve RDAP directly at `<base>/domain/...`.
5. Write a redirect template (same shape the server's own bootstrap writer uses):

   ```json
   {
     "domain": {
       "error": {
         "rdapConformance": ["rdap_level_0"],
         "notices": [
           {
             "links": [
               {
                 "value": "https://rdap.nic.ch/",
                 "rel": "related",
                 "href": "https://rdap.nic.ch/",
                 "type": "application/rdap+json"
               }
             ]
           }
         ],
         "errorCode": 307
       }
     },
     "ids": [{ "ldhName": "ch" }]
   }
   ```

The `tld_redirect_` file-name prefix is deliberately distinct from the
`bootstrap_*` prefixes the server removes on each bootstrap cycle, so generated
files survive bootstrap reloads. Each run first clears its own previous
`tld_redirect_*.template` files, making the run idempotent.

## Usage

```sh
cargo run -- --data-dir /data
```

| Flag | Default | Description |
| --- | --- | --- |
| `--data-dir` | `srv/data` (env `RDAP_SRV_DATA_DIR`) | Directory to write redirect templates into |
| `--root-zone-url` | `https://www.internic.net/domain/root.zone` | Root zone file URL |
| `--exclude <tld>` | *(empty)* | TLD to skip from scanning. Repeatable |
| `--exclude-file <path>` | *(none)* | File of TLDs to skip, one per line (`#` comments allowed) |
| `--overrides-file <path>` | bundled `overrides.txt` | File mapping TLDs to explicit RDAP base URLs |
| `--concurrency` | `16` | Max concurrent probes |
| `--timeout-secs` | `10` | Per-request timeout |

### Exception list

TLDs that should never be scanned are configured via `--exclude` and/or
`--exclude-file`. The list is **empty by default**. Example file:

```
# TLDs to skip (one per line)
test
example
```

A TLD added to the exception list on a later run has any previously generated
redirect file removed (via the per-run clear).

### Overrides (non-pattern servers)

Some TLDs host their RDAP server at a host that the `nic.<tld>` probe will never
find. These are pinned to an explicit base URL in the bundled
[`overrides.txt`](overrides.txt), which is compiled into the binary (so the
default works from any directory). For a TLD listed there, the tool uses the given
base directly and does **not** probe.

Format — `<TLD><whitespace><RDAP base URL>` per line; blank lines and `#`
comments are ignored; the base URL is normalised to end with `/`:

```
.io	https://rdap.identitydigital.services/rdap
.me	https://rdap.identitydigital.services/rdap
.sh	https://rdap.identitydigital.services
.co	https://rdap.registry.co/co
.ru	https://cctld.ru/tci-ripn-rdap
.de	https://rdap.denic.de
```

Overrides only take effect for TLDs that are delegated in the root zone but absent
from the IANA bootstrap (the same set that would otherwise be probed). Supply a
different mapping at runtime with `--overrides-file <path>`.

## Verifying

After generating, point an `icann-rdap-srv` bootstrap server at the data
directory and query a domain under a redirected TLD:

```sh
curl -i http://localhost:3000/rdap/domain/example.ch
# HTTP/1.1 307 Temporary Redirect
# ... link to https://rdap.nic.ch/domain/example.ch
```

## Tests

```sh
cargo test
```

Includes a round-trip test that writes a redirect file, loads it through the real
`icann-rdap-srv` storage loader, and asserts the domain resolves to a `307`
redirect with the expected target.
