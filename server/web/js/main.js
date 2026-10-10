// Starts the web client (docs/plans/030_web_client.md): the state, the player,
// the window, then a sync.

import { App } from "./app.js";
import { Playback } from "./playback.js";
import { Shell } from "./ui/shell.js";
import { applyTheme } from "./ui/settings.js";
import { ctx } from "./ui/ctx.js";

const app = new App();
ctx.app = app;
applyTheme(app.prefs);
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () =>
  applyTheme(app.prefs),
);
ctx.playback = new Playback(app);
// What was kept is loaded first, so a reload on an album's address finds the album.
await app.start();
new Shell(document.body);
if (app.signedIn) {
  app.checkHandoff();
  app.syncNow();
}

// When sync runs, besides after each edit: the page coming back into view,
// and the network returning (plan 030).
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    app.checkHandoff();
    app.soon(0);
  } else {
    // Hidden is not closed: a background tab plays on, so only what would be lost is saved.
    ctx.playback.saveListen();
    app.flush();
  }
});
addEventListener("online", () => app.soon(0));
addEventListener("pagehide", () => {
  ctx.playback.leaving();
  app.flush();
});
