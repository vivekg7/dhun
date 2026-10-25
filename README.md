# Dhun

Self-hosted music, podcasts and audiobooks for a family: a lightweight Go
server for a NAS, and native clients for Android (full-featured, primary),
macOS and Linux, plus a web client, with **multiple named queues**, offline
downloads and synced lyrics. Inspired by Musicolet, the offline Android
player, rebuilt for a music collection that lives on a NAS.

**Status:** every platform is in the
[0.18.0 release](https://github.com/vivekg7/dhun/releases/tag/v0.18.0): the
APK, the Mac app, the Linux Flatpak and the server image.

- **Server** (`server/`), on the owner's NAS as `server-v0.16.0`: the library
  scan, streaming, offline sync of queues and playlists (`.m3u8` files), a
  log of every listen, resume points for long files, Favorites and Listen
  Later.
- **Android** (`android/`, [plan 011](docs/plans/011_android_app.md)), 0.16.2
  on the owner's phone: downloads, playlist editing, the sleep timer,
  lyrics, speed and pitch, hand-off, a song cache, playback that waits out a
  network drop ([019](docs/plans/019_networking_and_caching.md)),
  Musicolet's Queues tab ([020](docs/plans/020_queue_screen.md)), files
  opened from other apps ([021](docs/plans/021_open_from_other_apps.md)), a
  choice of mini player ([022](docs/plans/022_mini_player_styles.md)),
  managing family members ([023](docs/plans/023_users_on_android.md)), and
  songs already on the phone ([025](docs/plans/025_phone_local_songs.md)),
  playable without signing in ([026](docs/plans/026_without_an_account.md)).
- **macOS** (`macos/`, [plan 024](docs/plans/024_macos_app.md)), 0.16.1 on
  the owner's Mac: the Android app's level, offline included, with a menu
  bar control, a floating mini player and Open With from Finder.
- **Linux** (`linux/`, [plan 039](docs/plans/039_linux_app.md)), 0.17.1 on
  the owner's laptop, in Rust and GTK: the same level, with a tray icon, a
  mini window, the desktop's media controls and Open With from the file
  manager.
- **Web** (`server/web/`, [plan 030](docs/plans/030_web_client.md)), served
  by the server at `http://<nas>:8585/`: the apps' features in a browser,
  online only, on a computer or a phone.

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
5. Add each family member from Settings → Family members, signed in as the
   admin, in any of the apps or the web client. Their playlists go in
   `Music/Playlists/<name>/`.

Every step in detail for Synology, why it is set up this way and what
protects the collection:
[`docs/plans/003_deployment.md#install`](docs/plans/003_deployment.md#install).

Forgot the admin password? `docker exec -it dhun /dhun user passwd <name>`.

## Develop

`make init` once per clone, then `make check` (formatting, linters, tests),
`make fmt`, and `make image`. The API contract is
[`api/openapi.yaml`](api/openapi.yaml).

- **Android** needs a JDK (Android Studio's is found on its own) and
  `android/local.properties` pointing at an Android SDK with platform 37.
- **macOS** needs Xcode for its toolchain only: the app is a Swift package,
  built from the command line.
- **Linux** needs Rust and the GTK 4, libadwaita, GStreamer and SQLite
  `-dev` packages, or else only Flatpak: without `cargo` on the PATH the
  Makefile runs it inside the GNOME SDK (`scripts/sdk.sh` says what to
  install once). `DHUN_SCREENSHOT=<png>` with `DHUN_OPEN` draws the window
  to a file and quits, for checking a change with no one at the screen
  ([plan 039](docs/plans/039_linux_app.md)).
- **Web** needs nothing to build: plain HTML, CSS and JavaScript, embedded
  in the server. Run the server with `DHUN_WEB_DIR=server/web` so a reload
  shows an edit without a rebuild; `make test-web` runs its tests with
  Node's own runner.

A Mac or Linux build under test takes `DHUN_DATA=<folder>`, so it keeps its
own library and sign-in instead of the installed app's. The apps log how long
the catalogue takes, for the speed targets
([performance-baseline](docs/research/performance-baseline.md)).

### Versions

The server and every app share **one sequence of version numbers**; the web
client is part of the server and takes its number:

- A release that changes only some platforms takes the next **patch**
  number, for those platforms alone. The others skip that number for
  good: after an Android-only 0.12.1, the next server or Mac release is
  0.12.2 at the earliest.
- A **minor** or **major** release is for every platform together, all
  taking the same new number.

So a number belongs to one release only, and builds with the same number
came out together.

Until 0.12.0 each platform counted on its own (the server reached
`server-v0.1.5`, the Android app 0.11.0, the Mac app 0.3.0), and the numbers
said nothing about which builds went together. All three jumped to 0.12.0,
the first number none had used (owner, 2026-10-10). Linux joined at 0.17.0.

The Android version is in `android/app/build.gradle.kts` (raise
`versionCode` too), the Mac's in `macos/Info.plist` (raise
`CFBundleVersion` too), the Linux app's in `linux/app/Cargo.toml`, and the
server's is its tag, `server-vX.Y.Z`.

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

`make linux` (`scripts/build-linux.sh`) builds the Linux app's Flatpak and
archives it in `local/` as `dhun-linux-v<version>.flatpak`, under the same
naming rules; `--install` also installs it for this user, and
`flatpak install --user <file>` does on another machine. `make
linux-install` builds against the system's GTK instead and installs into
`~/.local`.

A GitHub release is tagged `vX.Y.Z` on the commit the version came from,
beside its `server-vX.Y.Z`, and carries the APK, the Mac zip and the Linux
Flatpak with their SHA-256 sums; the server ships as its image on ghcr.io.
The R8 mapping stays in `local/`: it is only for reading the owner's crash
reports.

- What it must do — [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)
- Why it is built this way — [`docs/plans/`](docs/plans/README.md)
- Projects we learn from — [`docs/INSPIRATIONS.md`](docs/INSPIRATIONS.md)
- Working on it (people and agents) — [`AGENTS.md`](AGENTS.md)

Licensed under the [GNU GPL v3.0](LICENSE).
