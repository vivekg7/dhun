# Dhun

Self-hosted music for a family: a lightweight Go server for a NAS, and native
clients for Android (full-featured, primary), macOS and the web — with
**multiple named queues**, offline downloads and synced lyrics. Inspired by
Musicolet, the offline Android player, rebuilt for a music collection that
lives on a NAS.

**Status:** the server works (library scan, streaming, sync, playlists as
`.m3u8` files). The Android app is next; macOS and web come after.

## Run the server

On the NAS (Synology Container Manager or any Docker host):

1. Create a project folder with a `data` folder in it, as your own user (on
   Synology: `docker/dhun/data` in File Station).
2. Copy [`deploy/docker-compose.yml`](deploy/docker-compose.yml) into the
   project folder and edit the lines marked `EDIT`: your `Music` path (it is
   mounted read-only, apart from `Playlists/`), your user ID, and the admin's
   name and password. That account is created on first start.
3. Start the project (Container Manager → Project → Create). The log should
   say `scan done … failed=0`. Then delete the password line; it is never
   read again.
4. Open `http://<nas>:8585` at home, or the NAS's Tailscale address away.
5. Add each family member from the admin panel. Their playlists go in
   `Music/Playlists/<name>/`.

Why it is set up this way, and what protects the collection:
[`docs/plans/003_deployment.md`](docs/plans/003_deployment.md).

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
