---
name: test-account
description: "Test the apps against the production NAS with the test account in local/test_account, never the owner's"
metadata:
  node_type: memory
  type: project
  originSessionId: c19a6fd7-d2e4-40dc-aee4-68b293859306
  modified: 2026-10-11T04:46:30.546Z
---

The owner made a Dhun account on the production NAS for testing (2026-10-11).
Its credentials are in `local/test_account`, which is git-ignored. Sign in with
it to test the web client or any app, and never use the owner's own
account. Playing anything on the owner's account writes listens, queues
(evicting one at the 20-queue limit), played marks and settings to their real
data.

**Why:** a test session on the owner's account in October 2026 evicted one of
their queues and filled their listening history.

**How to apply:** read the file when a test needs to sign in. Never print or
commit its contents. Anything the test account changes is fine to leave.
