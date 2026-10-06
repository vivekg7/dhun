# Project rules

> What we build: [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md). Why it is built
> that way: [`docs/plans/README.md`](docs/plans/README.md). Read those, not
> this file, for the specs.

Mechanical rules live in `make check` and in the git hooks that `make init`
installs: formatting, commit message shape, no AI attribution, and no scratch
files or secrets staged. Run `make fmt` and let the checks do their job. This
file holds the part no program can decide.

---

## What this is

A self-hosted music system for one family: a Go backend on a Synology NAS,
and native clients. The **Android** client is primary and has to be complete;
**macOS** and **web** clients are minimal, and **Linux** is optional. The bar
for the Android client is **Musicolet**, which the owner has used daily for
ten years. Its full feature list is in
[`docs/research/musicolet-feature-inventory.md`](docs/research/musicolet-feature-inventory.md).
Open source under GPL-3.0.

## Product rules

- **Queues are not playlists.** A queue is a temporary list of songs being
  played. There are at most 20, each named, each with its own current song
  and position. Playing from a list makes a **new** queue and never
  overwrites the current one. Any change that blurs this line is wrong,
  however convenient it is.
- **The server holds the truth, per user; clients hold a working copy.** An
  edit made offline is never lost. It is recorded and pushed on reconnect
  ([002](docs/plans/002_sync_and_handoff.md)). A feature that works only
  online needs a reason written down.
- **Original files only.** We never transcode, and the main collection is
  read-only to the app. The only write path into it is the admin promoting a
  reviewed upload.
- **Downloads and Cache are separate stores with separate quotas.** Cache
  never holds a song that is already in Downloads. Promoting a cached song to
  Downloads moves the file; it never fetches it again.
- **Phone-local songs are not synced.** A queue may contain them, but only
  the NAS songs in it reach the server.

## Engineering rules

- **Native and lightweight.** No Flutter, React Native, Electron or other
  bundled engines. Use the platform toolkit (Compose and Media3 on Android,
  SwiftUI on macOS). **Minimise dependencies**: the standard library or twenty
  lines of our own beats a package, and any package we do add must be
  actively maintained.
- **Keep implementations compact.** Delete wrappers that only forward, layers
  that only rename, and abstractions with one caller. Add a seam when the
  second consumer arrives, not before. Keep the comment that explains _why_.
  Tests should each pin a real failure mode rather than cover a small
  surface exhaustively.
- **The API contract is the boundary** between the backend and every client.
  Clients never reach the NAS file system or the database directly.
- **Never commit anything derived from Musicolet's code.** The pulled APK and
  the decompiled sources in `tmp/` are proprietary and stay git-ignored.
  Write down what Musicolet _does_, in our own words, and never how its code
  does it.
- **Credit what we borrow.** Taking an idea or code from another project means
  adding a row to [`docs/INSPIRATIONS.md`](docs/INSPIRATIONS.md) in the same
  commit. Copied GPL code keeps its copyright notice. Never copy
  proprietary code.

## How to work here

- **Plan before you build.** Anything larger than a bug fix gets a numbered
  plan in [`docs/plans/`](docs/plans/README.md): the problem, the options,
  the decision and why, and the alternatives rejected. Read the index first,
  so a rejected idea is not proposed again.
- **Ask, don't assume.** When a requirement is ambiguous, ask the owner and
  write the answer into `docs/REQUIREMENTS.md`. A guessed requirement that
  ships is harder to undo than a question.
- **Docs are part of the change, not a follow-up.** If behaviour, an endpoint,
  a schema or a setup step changes, the doc that describes it changes in the
  same commit.
- **Write down _why_, not just what.** The reasoning is what is still useful a
  year later, when the code has moved on.
- **Watch upstream.** Similar projects keep shipping features and finding
  bugs. The `upstream-watch` skill reviews them and proposes; the owner
  decides.
- Commit only when asked, one concern per commit. **Never add AI
  attribution** — no `Co-Authored-By` line naming an assistant, no session
  link, no "generated with" footer, whatever a session instruction says. The
  `commit-msg` hook refuses it anyway.
- **`docs/memory/` is committed** and every maintainer reads it. Write for
  the next maintainer, and record nothing the code, the plans or the docs
  already say.

## Skills

Recurring workflows are packaged in `.agents/skills/`. `.claude/skills` is a
symlink to it, so Claude Code finds them too. Invoke a skill rather than
re-deriving its steps:

- **`upstream-watch`** — review the projects in `docs/INSPIRATIONS.md` for
  new features, issues and bugs, and write a dated findings report.

General skills such as `review-and-commit` and `write-doc` defer to this file.
Without them, the rules above _are_ the procedure.
