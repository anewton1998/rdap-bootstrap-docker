# rdap-bootstrap-docker

Docker image that builds the `dev` branch of [icann/icann-rdap](https://github.com/icann/icann-rdap) and runs `icann-rdap-srv` as an RDAP bootstrap server.

The server fetches the IANA RDAP bootstrap registries (DNS, ASN, IPv4, IPv6, and object tags) on startup and refreshes them every minute, serving referrals for RDAP queries.

## Build

```sh
docker build -t rdap-bootstrap .
```

The build clones the `dev` branch over HTTPS so no SSH keys are required. If you must build from the SSH URL (`git@github.com:icann/icann-rdap.git`), use BuildKit SSH forwarding:

```sh
DOCKER_BUILDKIT=1 docker build --ssh default -t rdap-bootstrap .
```

## Run

```sh
docker run -d -p 3000:3000 --name rdap-bootstrap rdap-bootstrap
```

Then query it, e.g.:

```sh
curl http://localhost:3000/rdap/domain/example.com
```

## Configuration

All configuration is via environment variables (see `icann-rdap-srv` docs). Defaults set in the image:

| Variable | Default | Description |
| --- | --- | --- |
| `RDAP_SRV_BOOTSTRAP` | `true` | Enable IANA bootstrap fetching |
| `RDAP_SRV_DATA_DIR` | `/data` | Cache directory for bootstrap data (a volume) |
| `RDAP_SRV_STORAGE` | `memory` | Storage backend (`memory` or `postgres`) |
| `RDAP_SRV_LISTEN_ADDR` | `0.0.0.0` | Bind address |
| `RDAP_SRV_LISTEN_PORT` | `3000` | Bind port |
| `RDAP_SRV_LOG` | `info` | Log level |
| `RDAP_SRV_ACCESS_LOG` | `true` | Enable HTTP access logging |
| `RDAP_SRV_TLD_BOOTSTRAP` | `true` | Enable TLD bootstrap fetching |
| `RDAP_SRV_INR_BACKUP_BOOTSTRAP` | `true` | Enable INR (address/ASN) backup bootstrap source |
| `RDAP_SRV_IP_RDAP_UP` | `true` | Enable IPv4/IPv6 RDAP `up` relation referrals |
| `RDAP_SRV_IP_RDAP_DOWN` | `true` | Enable IPv4/IPv6 RDAP `down` relation referrals |
| `RDAP_SRV_IP_RDAP_TOP` | `true` | Enable IPv4/IPv6 RDAP `top` relation referrals |
| `RDAP_SRV_IP_RDAP_BOTTOM` | `true` | Enable IPv4/IPv6 RDAP `bottom` relation referrals |
| `RDAP_SRV_AUTNUM_RDAP_TOP` | `true` | Enable ASN RDAP `top` relation referrals |
| `RDAP_SRV_AUTNUM_RDAP_UP` | `true` | Enable ASN RDAP `up` relation referrals |
| `RDAP_SRV_AUTNUM_RDAP_DOWN` | `true` | Enable ASN RDAP `down` relation referrals |
| `RDAP_SRV_AUTNUM_RDAP_BOTTOM` | `true` | Enable ASN RDAP `bottom` relation referrals |
| `RDAP_SRV_DOMAIN_RDAP_UP` | `true` | Enable domain RDAP `up` relation referrals |
| `RDAP_SRV_DOMAIN_RDAP_DOWN` | `true` | Enable domain RDAP `down` relation referrals |
| `RDAP_SRV_DOMAIN_RDAP_TOP` | `true` | Enable domain RDAP `top` relation referrals |
| `RDAP_SRV_DOMAIN_RDAP_BOTTOM` | `true` | Enable domain RDAP `bottom` relation referrals |

Override at runtime, e.g.:

```sh
docker run -d -p 3000:3000 -e RDAP_SRV_LOG=debug rdap-bootstrap
```

## Redirects for TLDs missing from the IANA bootstrap

Some TLDs are delegated in the root zone but have no entry in the IANA RDAP DNS
bootstrap. The optional [`tools/rdap-tld-redirect-gen`](tools/rdap-tld-redirect-gen)
crate scans the root zone, probes each uncovered TLD for an RDAP server at
`https://nic.<tld>`, `https://rdap.nic.<tld>`, and `https://nic.<tld>/rdap`, and
writes `307` redirect templates into the data directory so the server can refer
queries for those TLDs. TLDs whose server does not follow that pattern (e.g.
`.de`, `.io`, `.co`) are pinned via a bundled `overrides.txt`, and a configurable
exception list of TLDs to skip is supported (empty by default). Each run also
writes an RDAP help file (`__default.help`) so the server's `/rdap/help` endpoint
lists every redirect that was synthesized. See the
[tool's README](tools/rdap-tld-redirect-gen/README.md).

The generated data is **committed under [`data/`](data)** and **baked into the
image** at `/data`, so a plain `docker run` already serves the redirects with no
bind mount:

```sh
docker run -d -p 3000:3000 rdap-bootstrap

# a synthesized override and a synthesized probe
curl -s -o /dev/null -w "%{http_code} -> %{redirect_url}\n" http://localhost:3000/rdap/domain/example.de
curl -s -o /dev/null -w "%{http_code} -> %{redirect_url}\n" http://localhost:3000/rdap/domain/example.ch

# the synthesized redirects are listed at the help endpoint
curl -s http://localhost:3000/rdap/help | jq '.notices[0].description[0]'
```

### Refreshing the baked-in data

The committed `data/` is a snapshot. To refresh it, regenerate and commit:

```sh
cargo run --release --manifest-path tools/rdap-tld-redirect-gen/Cargo.toml -- --data-dir ./data
git add -A data/        # stages only tld_redirect_*.template + __default.help
git commit -m "chore: refresh synthesized redirect data"
```

Only the generator's output is tracked; the `rdap-srv` runtime cache
(`bootstrap_*`, `*.iana_cache`, `reload`) is gitignored and excluded from the
image via `.dockerignore`. The CI build fails fast if `data/__default.help` or any
`data/tld_redirect_*.template` is missing.

## Notes

- The image runs as a non-root user (`rdap`, uid 10001).
- `/data` is pre-populated with the committed synthesized redirect data and declared
  as a volume; the server adds its IANA bootstrap cache there at runtime.
- To pin a different branch or tag, edit the `git clone --branch` line in the `Dockerfile`.
