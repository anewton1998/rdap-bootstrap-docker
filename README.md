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

Override at runtime, e.g.:

```sh
docker run -d -p 3000:3000 -e RDAP_SRV_LOG=debug rdap-bootstrap
```

## Notes

- The image runs as a non-root user (`rdap`, uid 10001).
- `/data` is declared as a volume to persist the IANA bootstrap cache between runs.
- To pin a different branch or tag, edit the `git clone --branch` line in the `Dockerfile`.
