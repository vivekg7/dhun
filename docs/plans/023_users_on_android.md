# 023 — Managing family members on Android

**Status:** `ACCEPTED` — moved from the web client to Android; no code yet
**Started:** 2026-10-08

## Problem

Family members are added only by the admin; there is no sign-up
([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)). Plan
[007](007_client_architecture.md) put the screen for it, **Users**, in the
web client's admin panel, and v1 listed it as web only. The server side has
been on the NAS since `server-v0.1.0`: list users, add a member, reset a
password ([006](006_api_and_sync.md)). But the web client is not started,
so today adding a member or resetting a forgotten password means `curl` and
a device token ([003](003_deployment.md)). The screen waits on a whole
client that has no other v1 work the family is waiting for.

## Options

- **On Android only** _(chosen)_: one more page under Settings, shown only
  to the admin.
- **Web only, as planned.** Rejected: it ties a small, needed screen to
  building the web client first, and the owner always has the phone to
  hand, not always a computer.
- **Both.** Rejected for now: two copies of the same three actions to keep
  in step, for one admin. The web admin panel can add Users later if a
  reason appears.

Plan 007's reason for the web — "a large screen suits reviewing an inbox"
— is about curation ([004](004_curation_workflow.md)), and still holds for
it. Users is a short list, a form and one action; it does not need the
large screen. It went to the web along with the rest of the panel, not for
its own sake.

## Decision

The owner chose Android only (2026-10-08). The curation screens stay on
the web, after v1, as before. The rest was decided while planning, and the
owner may override it:

- **No API change.** Login and `/api/v1/me` already return `user.admin`,
  and the admin endpoints already refuse anyone else (403). The app keeps
  the flag with the user's name. A phone signed in before this release has
  no flag stored, so the app asks `/me` once to fill it in.
- **Under Settings → Account**, a row "Family members" for the admin only,
  opening a page of its own ([018](018_settings_layout.md)). Not a new
  category: there is one admin, and the first screen stays as everyone
  else sees it.
- **The page** lists every user, with how many devices are signed in and
  when one was last seen, which the list endpoint already returns. That
  answers "did they ever sign in?" without asking them.
- **Add a member:** a dialog with a name and a password. The server's
  rules (a name usable as a folder name, eight characters of password, no
  second admin, names not repeated ignoring case) show as its error under
  the field, rather than copied into the app, so they cannot drift.
- **Reset a password:** tap a member, then a dialog for the new password.
  It says first that every device of theirs is signed out, since the
  server does that.
- **A signed-out device keeps its unsynced edits.** Today a 401 during
  sync calls the same sign-out as the Sign out button, which clears the
  database, the outbox of unpushed edits with it. A password reset is
  exactly that 401, so a member who edited a playlist offline would lose
  the edit: the rule that an offline edit is never lost
  ([002](002_sync_and_handoff.md)) would break the first time this screen
  is used. So a revoked token no longer signs out. The app keeps
  everything, playback and downloads included, and asks for the password
  again, the name already filled in; the outbox is pushed once it is
  given. Only signing in as a different user, or the Sign out button,
  clears the phone. The bug is older than this plan, but this screen
  turns it from rare into one tap.
- **The admin's own row cannot be reset here.** Resetting it would sign
  out this phone too, and signing out deletes its downloads. Changing the
  admin's own password stays `dhun user passwd` on the NAS, the way back
  in already documented in plan 006.
- **Online only.** These are admin actions on the server, not part of the
  user's synced data ([002](002_sync_and_handoff.md)): offline, the page
  says it needs the server rather than queueing a new account to create
  later.
- **No delete,** as the server has none: a user's playlists, queues and
  plays would need a decision first (006).

## Plan

1. Fix the revoked token first, in its own commit: on a 401 keep the
   data and ask for the password again, as above. It must land before the
   screen does.
2. Keep `admin` beside `userName` in the app's stored preferences, set at
   sign-in and filled from `/me` when missing.
3. The Family members page and its two dialogs, using the shared dialog
   pieces in `ui/Dialogs.kt`.
4. Update `docs/REQUIREMENTS.md` (v1 scope) and plan 007 to say Users is on
   Android, in the same commit as this plan.

The part that can fail is step 1: test it by editing a playlist offline on
one phone, resetting that user's password from another, reconnecting, and
checking the edit reaches the server once the new password is entered.

## Open questions

None.
