# 023 — Managing family members on Android

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.8.0 on the owner's phone
**Started:** 2026-10-08

## Problem

Family members are added only by the admin; there is no sign-up
([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)). Plan
[007](007_client_architecture.md) put the screen for it, **Users**, in the
web client's admin panel, and v1 listed it as web only. The server side
(list users, add a member, reset a password, [006](006_api_and_sync.md))
has been on the NAS since `server-v0.1.0`, but the web client was not
started, so adding a member or resetting a forgotten password meant `curl`
and a device token ([003](003_deployment.md)). The screen waited on a whole
client with no other v1 work the family was waiting for.

## Options

- **On Android only** _(chosen)_: one more page under Settings, shown only
  to the admin.
- **Web only, as planned.** Rejected: it ties a small, needed screen to
  building the web client first, and the owner always has the phone to
  hand, not always a computer.
- **Both.** Rejected at the time: two copies of the same three actions to
  keep in step, for one admin. The reason went once every client was built
  to the phone's level: the Mac ([024](024_macos_app.md)), the web
  ([030](030_web_client.md)) and Linux ([039](039_linux_app.md)) now have
  the same page.

Plan 007's reason for the web — "a large screen suits reviewing an inbox"
— is about curation ([004](004_curation_workflow.md)), and still holds for
it. Users is a short list, a form and one action, and went to the web with
the rest of the panel, not for its own sake.

## Decision

The owner chose Android only (2026-10-08). The curation screens stay on
the web, after v1, as before. The rest was decided while planning, and the
owner may override it:

- **No API change.** Login and `/api/v1/me` already return `user.admin`,
  and the admin endpoints refuse anyone else (403). The app keeps the flag
  with the user's name; a phone signed in before this release asks `/me`
  for it when it next comes to the foreground, as for a missing device id.
- **Under Settings → Account**, a row "Family members" for the admin only,
  opening a page of its own ([018](018_settings_layout.md)). Not a new
  category: there is one admin, and the first screen stays as everyone
  else sees it.
- **The page** lists every user, with how many devices are signed in and
  when one was last seen, as the list endpoint returns: that answers "did
  they ever sign in?" without asking them.
- **Add a member:** a dialog with a name and a password. The server's
  rules (a name usable as a folder name, eight characters of password, no
  second admin, names not repeated ignoring case) show as its error under
  the field rather than being copied into the app, so they cannot drift.
- **Reset a password:** tap a member, then a dialog for the new password,
  which says that every device of theirs is signed out, as the server does.
- **Passwords are shown as typed** in both dialogs. The admin reads the
  password out to the member, and a typo in a hidden one would lock them
  out.
- **A signed-out device keeps its unsynced edits.** A 401 during sync
  called the same sign-out as the Sign out button, which cleared the
  database and its outbox of unpushed edits. A password reset is exactly
  that 401, so a member's offline playlist edit would be lost, breaking
  the rule that an offline edit never is ([002](002_sync_and_handoff.md))
  the first time this screen was used. So a revoked token no longer signs
  out: the app keeps everything, playback and downloads included, asks for
  the password again with the name filled in, and pushes the outbox once
  it is given. Only signing in as another user, or Sign out, clears the
  phone. The bug is older than this plan; this screen turned it from rare
  into one tap.
- **The admin's own row cannot be reset here.** It would sign out this
  phone too, and signing out then deleted its downloads (since
  [026](026_without_an_account.md) it can keep them). The admin's own
  password stays `dhun user passwd` on the NAS, the way back in that plan
  006 documents.
- **Online only.** These are admin actions on the server, not the user's
  synced data ([002](002_sync_and_handoff.md)): offline, the page says it
  needs the server rather than queueing an account to create later.
- **No delete,** as the server has none: a user's playlists, queues and
  plays would need a decision first (006).

## Plan

1. Fix the revoked token first, in its own commit, before the screen
   lands: on a 401 keep the data and ask for the password again.
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
