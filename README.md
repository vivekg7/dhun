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

1. Copy [`deploy/docker-compose.yml`](deploy/docker-compose.yml). Edit the
   line marked `MEDIA_PATH` to point at your `Music` folder, and set
   `DHUN_ADMIN_USER` and `DHUN_ADMIN_PASSWORD`: that account, the admin, is
   created on first start. Dhun keeps its own data in `Music/_dhun/`.
2. `docker compose up -d`. Once it is running, you can remove the admin
   password from the file; it is never read again.
3. Add each family member from the admin panel. Their playlists go in
   `Music/Playlists/<name>/`.
4. Reach it at `http://<nas>:8585` over Tailscale.

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
