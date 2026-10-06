# Existing servers and clients

Survey of what already exists, to decide between forking a client and building
our own. Researched 2026-10-06; activity dates come from GitHub/GitLab.
Items marked **(unverified)** were not confirmed from a primary source.

## Key findings

1. **Nobody has multiple named queues except Musicolet and Symfonium** (closed
   source; keeps the last 15 queues, each with its own position, shuffle,
   repeat and speed). No open-source client has them.
2. **No server stores more than one queue per user.** Subsonic
   `savePlayQueue`, and OpenSubsonic's `indexBasedQueue` extension
   (`savePlayQueueByIndex`), both hold a single queue. None of OpenSubsonic's
   11 extensions covers named queues. If queues must sync between devices, we
   either store them as hidden/prefixed playlists plus a position map, or
   propose a new OpenSubsonic extension.
3. **Our two differentiators — multiple queues and Downloads + Cache with
   separate quotas — are core architecture.** Any fork would need them
   retrofitted, which removes most of the benefit of forking.
4. **Ultrasonic already implements "cached song becomes a download without
   re-downloading":** pinning renames the cached file into the pinned folder.
   That is exactly our Cache → Downloads rule.

## Servers (run on the Synology NAS)

| Server                 | API                                  | Licence       | Real folder tree                     | Synced lyrics                                           | Notes                                                                                                                                                             |
| ---------------------- | ------------------------------------ | ------------- | ------------------------------------ | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Navidrome**          | Subsonic + OpenSubsonic (broad)      | GPL-3.0, Go   | **No** — deliberately tag-based only | Yes (embedded, `.lrc`, word-timed sidecars since v0.63) | Most popular, very active, native SynoCommunity package, low resource use, on-the-fly transcoding. Clients can still build a folder view from each song's `path`. |
| **Gonic**              | Subsonic + partial OpenSubsonic      | GPL-3.0, Go   | **Yes**                              | Yes                                                     | Very light, active.                                                                                                                                               |
| **LMS**                | Subsonic + OpenSubsonic              | GPL-3.0, C++  | Yes                                  | Yes (`.lrc`, embedded)                                  | Very light, active.                                                                                                                                               |
| Ampache                | Own + Subsonic + widest OpenSubsonic | AGPL-3.0, PHP | Yes (unverified)                     | Yes, word-level                                         | Needs MariaDB/MySQL. Only server confirmed to support `indexBasedQueue`.                                                                                          |
| Jellyfin               | Own REST API, not Subsonic           | GPL-2.0, C#   | Yes                                  | Yes (line-level)                                        | Heaviest. A different API to target.                                                                                                                              |
| Koel                   | Own API; Subsonic since v9.5         | MIT, PHP      | unverified                           | Yes                                                     | Needs a database.                                                                                                                                                 |
| Funkwhale 2.0          | Own API + Subsonic subset            | AGPL-3.0      | Partial                              | Limited (unverified)                                    | Postgres + Redis + Celery; overkill.                                                                                                                              |
| Airsonic-Advanced      | Subsonic                             | GPL-3.0, Java | Yes                                  | Basic                                                   | Stale since 2024-04. Avoid.                                                                                                                                       |
| Synology Audio Station | Proprietary, undocumented            | Closed        | Yes                                  | Plugins                                                 | Bad base for an open-source client.                                                                                                                               |

**Default recommendation:** target **OpenSubsonic generically** so any of the
first three works. Navidrome is the low-friction pick; Gonic or LMS if the
real folder tree must come from the server.

## Android clients (open source, native)

| Client                       | Stack                               | Licence         | Activity                        | Offline                                       | Lyrics             | Verdict                                                          |
| ---------------------------- | ----------------------------------- | --------------- | ------------------------------- | --------------------------------------------- | ------------------ | ---------------------------------------------------------------- |
| **Navic**                    | Kotlin, Compose, Media3, Ktor, Room | GPL-3.0         | Active, ~1.1k★                  | Offline library; cache/download split unknown | Word-by-word       | Best fork candidate if GPL is fine. One maintainer.              |
| **Tempus** (Tempo fork)      | ~82 % Java, XML Views, Media3       | GPL-3.0         | Very active (v4.28, 2026-10-02) | Separate download store and LRU stream cache  | Synced             | Most complete, but large and mostly Java. Reference, not a base. |
| **Ultrasonic**               | Kotlin, XML Views, Media3           | GPL-3.0         | Active (GitLab)                 | **Cache + pin-by-rename**                     | Plain only         | Copy its offline design.                                         |
| Youamp                       | Kotlin, Compose, Media3             | **MIT**         | Active                          | None                                          | —                  | Clean and small; base only if we want a permissive licence.      |
| Chora                        | Kotlin, Compose                     | Apache-2.0      | Active                          | Downloads + cache                             | Synced, word-level | Author calls the code messy.                                     |
| DSub / DSub2000              | Java, legacy                        | GPL-3.0         | Low                             | Cache + pin                                   | Basic              | Too old.                                                         |
| Sonora, Resonance, Vibrdrome | Kotlin, Compose                     | MIT / GPL / MIT | 2026, very new                  | —                                             | —                  | Ideas only.                                                      |

Excluded by the "no Flutter / no engine" rule: Finamp (Flutter), Castafiore
(React Native), Feishin and Sonixd (Electron). Feishin is still a good
reference for the web/desktop UI.

## macOS and code sharing

- Native Apple clients: **Submariner** (macOS, AppKit, BSD-3 — permissive),
  Amperfy (Swift, iOS + macOS, GPL-3.0), flo (SwiftUI, MIT).
- **Kotlin Multiplatform** can share a non-UI core (API client, models, queue
  and offline logic, sync, database) between Android and macOS; on macOS it
  is consumed from Swift as a framework. Compose Multiplatform for the desktop
  UI ships a JVM (~100 MB app, ~100 MB idle RAM — old community figures,
  unverified for 2026), which conflicts with "lightweight", so a **SwiftUI**
  UI on top of the shared core fits better. Compose for Web (Wasm) is beta.
- MIT KMP Subsonic libraries exist: `subsonic-kotlin` (zt64, OpenSubsonic,
  1.0.0-beta) and siper's `subsonic-api`.
- Rust is common for desktop-only clients, but no shared Rust core with an
  Android client was found.

## Licence implications

Copying code from Navic, Tempus, Ultrasonic, DSub, Amperfy or Feishin makes
our app **GPL-3.0**. Youamp, Sonora, Vibrdrome and flo are MIT; Chora is
Apache-2.0; Submariner is BSD-3. Reading GPL code for design ideas does not
impose the licence; copying it does.

> **Update 2026-10-06:** decided to write our own backend instead of using an
> existing server — see [REQUIREMENTS.md](../REQUIREMENTS.md). The server
> survey above stays as reference (scanning, lyrics and transcoding approaches
> worth borrowing).

## Fork vs build — preliminary assessment

**Leaning: build from scratch** with a shared Kotlin Multiplatform core, a
Compose + Media3 Android app, a SwiftUI macOS app and a thin web client.
Borrow designs from Ultrasonic (offline), Tempus/Navic (ReplayGain, lyrics,
Android Auto). Reason: the features that justify the project are
architectural, and the forkable candidates are either mostly Java or
single-maintainer.

Not decided — depends on the open questions in
[REQUIREMENTS.md](../REQUIREMENTS.md), especially licence choice and how
much effort is acceptable before a usable v1.

## Not verified

- Whether Navidrome, Gonic and LMS implement `indexBasedQueue`.
- How Navic stores offline files; whether Tempus promotes cache → downloads
  without copying.
- Ultrasonic speed/pitch support; Chora's media stack.
- Koel and Ampache folder browsing; Funkwhale's Subsonic coverage.
- Actual RAM/CPU of each server on a low-power NAS.
- Compose Desktop size/RAM in 2026; Flutter overhead was not benchmarked.
