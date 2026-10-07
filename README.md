# Dhun

Self-hosted music for a family: a lightweight Go server for a NAS, and native
clients for Android (full-featured, primary), macOS and the web — with
**multiple named queues**, offline downloads and synced lyrics. Inspired by
Musicolet, the offline Android player, rebuilt for a music collection that
lives on a NAS.

**Status:** the server runs on the owner's NAS (`server-v0.1.1`). It covers the library
scan, streaming, offline sync of queues and playlists (`.m3u8` files), a log
of every listen, resume points for long files, and Favorites and Listen
Later. The apps are not built yet: Android is next, then macOS and web. Until
then the server is used through its API.

## Run the server

On the NAS (Synology Container Manager or any Docker host):

1. Create a project folder with a `data` folder in it, as your own user. On
   Synology: `docker/dhun/data` in File Station, then remove the ACL it
   inherits (step 2 of the detailed install below).
2. Copy [`deploy/docker-compose.yml`](deploy/docker-compose.yml) into the
   project folder and edit the lines marked `EDIT`: your `Music` path (it is
   mounted read-only, apart from `Playlists/`), your user ID, and the admin's
   name and password. That account is created on first start.
3. Start the project (Container Manager → Project → Create). The log should
   say `scan done … failed=0`, and `http://<nas>:8585/healthz` answer `ok`.
   Then delete the password line; it is never read again.
4. The apps connect to `http://<nas>:8585` at home, or the NAS's Tailscale
   address away.
5. Add each family member: today with `curl`, later from the web app's
   Users screen. Their playlists go in `Music/Playlists/<name>/`.

Every step in detail, for Synology:
[`docs/plans/003_deployment.md#install`](docs/plans/003_deployment.md#install).

Why it is set up this way, and what protects the collection: the same plan.

Forgot the admin password? `docker exec -it dhun /dhun user passwd <name>`.

## Develop

`make init` once per clone, then `make check` (formatting, `go vet`, tests),
`make fmt`, and `make image`. The API contract is
[`api/openapi.yaml`](api/openapi.yaml).

- What it must do — [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)
- Why it is built this way — [`docs/plans/`](docs/plans/README.md)
- Projects we learn from — [`docs/INSPIRATIONS.md`](docs/INSPIRATIONS.md)
- Working on it (people and agents) — [`AGENTS.md`](AGENTS.md)

Licensed under the [GNU GPL v3.0](LICENSE).
