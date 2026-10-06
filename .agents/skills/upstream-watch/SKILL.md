---
name: upstream-watch
description: Review the projects in docs/INSPIRATIONS.md for what changed since they were last reviewed — new releases, notable features, open issues and bugs that touch something we build — and write a dated findings report with a recommendation per item (adopt, watch, ignore). Use for "check upstream", "what's new in similar projects", "any features worth taking from Navidrome/Tempus/…", "review the inspirations", "monthly upstream review". Read-only towards the projects and towards our code — it proposes, it never implements.
---

# Upstream watch

Code is cheap; knowing what to build is not. This review keeps a steady stream
of proven ideas, and of other projects' bugs we should avoid repeating, coming
into the backlog. It makes no decisions.

## Inputs

- The **Watched** table in [`docs/INSPIRATIONS.md`](../../../docs/INSPIRATIONS.md):
  project, source, licence, `Last reviewed`.
- What we build: [`docs/REQUIREMENTS.md`](../../../docs/REQUIREMENTS.md) and
  the plans index [`docs/plans/README.md`](../../../docs/plans/README.md).
  Relevance is judged against these.

## Steps

1. For each watched project, collect what happened since its
   **`Last reviewed`** date:
   - **GitHub projects:** releases and changelogs
     (`api.github.com/repos/<owner>/<repo>/releases`), merged PRs with a
     user-facing effect, and new or highly upvoted issues
     (`/issues?since=…&sort=reactions`).
   - **GitLab (Ultrasonic):** the equivalent `gitlab.com/api/v4` endpoints.
   - **Closed source (Musicolet, Symfonium):** public changelog, docs and
     forum only. **Never** download, decompile or quote their code for this
     review.
2. Keep only items that touch something we have or plan to build. A feature
   for something we do not do (podcasts, video, and so on) is skipped
   silently.
3. Classify each kept item:
   - **adopt** — worth a plan or a backlog entry;
   - **watch** — interesting, but not yet;
   - **avoid** — a bug or design flaw we should not repeat. Cite the issue.
4. Write `docs/upstream/YYYY-MM-DD.md`: one section per project, each item
   with its link, one line on what it is, the classification, and _why_.
   End with a short **Recommended next steps** list.
5. Update `Last reviewed` in `docs/INSPIRATIONS.md` for every project
   actually reviewed. A project that could not be fetched keeps its old date
   and is named in the report as not reviewed.

## Rules

- **Propose, don't implement.** An `adopt` item becomes a plan or an entry in
  `docs/REQUIREMENTS.md` only when the owner agrees.
- **Licence check before suggesting code reuse.** GPL-3.0 and GPL-compatible
  permissive licences (MIT, BSD, Apache-2.0) are fine to copy from, with
  credit. Proprietary code never is.
- **Cite everything.** Every item links to its release, PR or issue. An
  unlinked claim is not a finding.
- Unauthenticated GitHub API calls are rate-limited (60/hour). Batch them,
  and say in the report if a limit cut the review short.
