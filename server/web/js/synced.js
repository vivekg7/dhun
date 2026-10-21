// The settings that follow the user to every device, with their defaults and
// choices (docs/plans/034_settings_text_defaults_reset.md). The phone and the
// Mac keep the same table, and make check fails when the defaults differ: the
// same stored value, missing, must mean the same everywhere. One to a line,
// which that check relies on.

const s = (name, def, options) => ({ name, def, options });

export const synced = {
  previousRestart: s("previous.restart", true, [true, false]),
  minMinutes: s("longFiles.minMinutes", 15, [5, 10, 15, 20, 30, 60]),
  resume: s("longFiles.resume", "auto", ["auto", "ask", "off"]),
  longPrevious: s("longFiles.previous", "keep", ["keep", "seek", "ignore", "songs"]),
  longNext: s("longFiles.next", "next", ["next", "seek"]),
  seekBack: s("longFiles.seekBack", 10, [5, 10, 15, 30, 60]),
  seekForward: s("longFiles.seekForward", 30, [10, 15, 30, 60, 120]),
  rewind: s("longFiles.rewind", 10, [0, 5, 10, 15, 30]),
  rewindAfter: s("longFiles.rewindAfter", 5, [1, 5, 10, 30, 60]),
  // How many minutes of a long file the phone and the Mac cache ahead; 0 is the whole file (plan 037).
  cacheAhead: s("longFiles.cacheAhead", 30, [10, 15, 30, 60, 0]),
  autoRemove: s("listenLater.autoRemove", true, [true, false]),
  finishedPercent: s("listenLater.finishedPercent", 90, [80, 90, 95, 100]),
  // Whether a show's page leaves out the episodes already played.
  hidePlayed: s("podcasts.hidePlayed", true, [true, false]),
};
