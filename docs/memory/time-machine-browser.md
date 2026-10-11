---
name: time-machine-browser
description: "the timemachine" means the owner's Time Machine browser-capture tool, used to open and check the web client in real Chrome
metadata:
  type: reference
---

"Time Machine" or "the timemachine" is the owner's own browser-capture tool,
checked out at `~/Developer/ivyhomes/time-machine` and installed as the
`timemachine` CLI. It records and drives a real Chrome session: navigate,
click, type, screenshot, and every request and console message.

It is not macOS's backup feature, and not a host on the network.

**How to apply:** to open or check the web client (`server/web`) in a
browser, use the `web-capture` skill, which drives `timemachine`. Sign in
with the test account only ([[test-account]]). The NAS serves the released
client, so to try the checkout's `server/web` against its library, serve
`web.Handler()` locally with `DHUN_WEB_DIR=server/web` and forward `/api/`
to the NAS: the client calls the API on its own origin.
