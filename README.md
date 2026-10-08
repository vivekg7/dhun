# Dhun

Self-hosted music for a family: a lightweight Go server for a NAS, and native
clients for Android (full-featured, primary), macOS and the web — with
**multiple named queues**, offline downloads and synced lyrics. Inspired by
Musicolet, the offline Android player, rebuilt for a music collection that
lives on a NAS.

**Status:** the server runs on the owner's NAS (`server-v0.1.5`). It covers the library
scan, streaming, offline sync of queues and playlists (`.m3u8` files), a log
of every listen, resume points for long files, and Favorites and Listen
Later. The Android app (`android/`, [plan 011](docs/plans/011_android_app.md))
runs on the owner's phone as 0.8.0, with downloads, playlist editing, the
sleep timer, lyrics, speed and pitch, hand-off, settings grouped as in
Musicolet, a song cache, playback that waits out a network drop
([plan 019](docs/plans/019_networking_and_caching.md)), Musicolet's
Queues tab ([plan 020](docs/plans/020_queue_screen.md)), audio files opened
from other apps ([plan 021](docs/plans/021_open_from_other_apps.md)), a
choice of mini player ([plan 022](docs/plans/022_mini_player_styles.md))
and, for the admin, managing family members
([plan 023](docs/plans/023_users_on_android.md)); there is no GitHub
release yet. The macOS app (`macos/`, [plan 024](docs/plans/024_macos_app.md))
is built to the Android app's level, offline included, with a menu bar
control, a floating mini player and Open With from Finder; it has not yet
been installed for daily use. The web client comes after it.

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

`make init` once per clone, then `make check` (formatting, `go vet`, ktlint,
Android lint, swift-format, tests), `make fmt`, and `make image`. The API
contract is [`api/openapi.yaml`](api/openapi.yaml). The Android app needs a
JDK (Android Studio's is found on its own) and `android/local.properties`
pointing at an Android SDK with platform 37. The macOS app needs Xcode for
its toolchain only: it is a Swift package, built from the command line.

### Release builds

A release APK is signed when `local/keystore.properties` exists at the
repository root:

```properties
storeFile=local/dhun-release.jks
storePassword=…
keyAlias=dhun
keyPassword=…
```

`local/` is gitignored, so neither the key nor its password reaches the
repository; without the file, the release build comes out unsigned. **The key
cannot be recovered.** Lose it and no installed copy of Dhun can be updated,
because Android refuses an update signed by another key, so keep a copy off
this machine.

`make apk` (`scripts/archive-apk.sh`) builds the release and archives it in
`local/` as `dhun-v<version>.apk`, with its `.sha256` and the R8 `mapping.txt`
that turns an obfuscated crash back into names. A dirty working tree is
archived as `-dirty-g<commit>`, so work in progress never takes a release's
name, and an existing archive is never overwritten without `--force`.

`make mac` (`scripts/build-mac.sh`) builds `Dhun.app`, signs it ad hoc (no
Apple account or certificate), and archives it in `local/` as
`dhun-mac-v<version>.zip`, under the same naming rules; `--install` also
copies it to `/Applications`. The version is in `macos/Info.plist`. On a
Mac other than the one that built it, allow the first launch under System
Settings → Privacy & Security.

- What it must do — [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)
- Why it is built this way — [`docs/plans/`](docs/plans/README.md)
- Projects we learn from — [`docs/INSPIRATIONS.md`](docs/INSPIRATIONS.md)
- Working on it (people and agents) — [`AGENTS.md`](AGENTS.md)

Licensed under the [GNU GPL v3.0](LICENSE).
