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
// The window first, so the page paints at once; the kept copy then lands in
// it, and a reload on an album's address reaches the album once it has.
const started = app.start();
new Shell(document.body);
await started;
app.opened();

// When sync runs, besides after each edit: the page coming back into view,
// a laptop waking with it in view, and the network returning (plans 030, 043).
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    app.opened();
  } else {
    // Hidden is not closed: a background tab plays on, so only what would be lost is saved.
    ctx.playback.saveListen();
    app.flush();
  }
});
// Waking from sleep fires no event of its own: a timer that missed a minute shows it.
let ticked = Date.now();
setInterval(() => {
  if (Date.now() - ticked > 60_000 && document.visibilityState === "visible")
    app.opened();
  ticked = Date.now();
}, 10_000);
addEventListener("online", () => app.soon(0));
addEventListener("pagehide", () => {
  ctx.playback.leaving();
  app.flush();
});
