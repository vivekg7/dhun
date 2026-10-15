// Now playing in its three forms: the bar along the bottom of the window, the
// phone's mini player, and the phone's Now playing page; with the speed and
// sleep panels, the hand-off and resume banners, and the lyrics.

import { h, clock } from "../util.js";
import { icon } from "../icons.js";
import { iconButton, popover, menuAt, reducedMotion, toast } from "./kit.js";
import { cover, songMenu } from "./songs.js";
import { parseLyrics, lineAt } from "../lyrics.js";
import { formatSpeed, minSpeed, maxSpeed } from "../queue.js";
import { chapterLabel, chaptersPanel, spokenFacts } from "./spoken.js";
import { ctx } from "./ctx.js";

// MARK: Controls shared by the bar and the page

/** Shuffle, previous, play or pause, next, repeat; for an episode or a book, Chapters and notes in Shuffle's place. */
function transport(compact = false) {
  const p = ctx.playback;
  const shuffle = iconButton("shuffle", "Shuffle", () => p.toggleShuffle(), "toggle");
  const chapters = iconButton("chapters", "Chapters and notes", () =>
    popover(chapters, chaptersPanel),
  );
  const prev = iconButton("previous", "Previous", () => p.previous(true));
  const play = h(
    "button.play",
    { type: "button", onclick: () => p.toggle() },
    icon("play"),
  );
  const next = iconButton("next", "Next", () => p.next(true));
  const repeat = iconButton("repeat", "Repeat", () => p.cycleRepeat(), "toggle");
  const el = h(
    "div.transport",
    compact ? null : shuffle,
    compact ? null : chapters,
    prev,
    play,
    next,
    compact ? null : repeat,
  );
  let wasPlaying = null;
  return {
    el,
    update() {
      const none = !p.current;
      for (const b of [shuffle, prev, play, next, repeat]) b.disabled = none;
      // A book is not shuffled (plan 031).
      const spoken = !!p.current?.spoken;
      shuffle.hidden = spoken;
      chapters.hidden = !spoken;
      shuffle.classList.toggle("on", p.shuffle);
      shuffle.title = p.shuffle ? "Shuffle is on" : "Shuffle";
      const r = p.repeat;
      repeat.classList.toggle("on", r !== "off");
      repeat.replaceChildren(icon(r === "song" ? "repeatOne" : "repeat"));
      repeat.title = {
        off: "Repeat",
        queue: "Repeating the queue",
        song: "Repeating this song",
      }[r];
      repeat.setAttribute("aria-label", repeat.title);
      if (wasPlaying !== p.isPlaying) {
        // Play and Pause turn into each other (plan 027).
        play.replaceChildren(icon(p.isPlaying ? "pause" : "play"));
        if (wasPlaying !== null && !reducedMotion())
          play.animate([{ transform: "scale(.85)" }, { transform: "scale(1)" }], 200);
        wasPlaying = p.isPlaying;
      }
      play.title = p.isPlaying ? "Pause" : "Play";
      play.setAttribute("aria-label", play.title);
    },
  };
}

/** The seek bar: dragging shows where it will land, and seeks on release. */
function seeker() {
  const p = ctx.playback;
  const range = h("input.seek", {
    type: "range",
    min: 0,
    max: 1,
    step: "any",
    value: 0,
    "aria-label": "Position",
  });
  const at = h("span.time.at", "0:00");
  const length = h("span.time.length", "0:00");
  let dragging = false;
  range.addEventListener("input", () => {
    dragging = true;
    at.textContent = clock(Number(range.value));
    paint();
  });
  range.addEventListener("change", () => {
    dragging = false;
    p.seek(Number(range.value));
  });
  const paint = () =>
    range.style.setProperty(
      "--fill",
      `${(Number(range.value) / Number(range.max || 1)) * 100}%`,
    );
  return {
    el: h("div.seeker", at, range, length),
    update() {
      const d = p.duration || 0;
      range.disabled = !p.current;
      range.max = Math.max(d, 1);
      length.textContent = clock(d);
      if (!dragging) {
        range.value = Math.min(p.position, d);
        at.textContent = clock(p.position);
      }
      paint();
    },
  };
}

/** Favorites or Listen Later for the song playing; a mark turned on pops, as on the phone. */
function markButton(kind) {
  const [on, off, add, remove] =
    kind === "fav"
      ? ["heart", "heartOutline", "Add to Favorites", "Remove from Favorites"]
      : ["later", "laterOutline", "Listen Later", "Remove from Listen Later"];
  const b = iconButton(off, add, () => {
    const s = ctx.playback.current;
    if (s) ctx.app.mark(kind, s.id, !ctx.app.isMarked(kind, s.id));
  });
  let last = null;
  return {
    el: b,
    update() {
      const s = ctx.playback.current;
      const marked = !!s && ctx.app.isMarked(kind, s.id);
      b.disabled = !s;
      b.classList.toggle("on", marked);
      b.replaceChildren(icon(marked ? on : off));
      b.title = marked ? remove : add;
      b.setAttribute("aria-label", b.title);
      if (last && last.song === s?.id && !last.marked && marked && !reducedMotion())
        b.animate(
          [
            { transform: "scale(1)" },
            { transform: "scale(1.35)" },
            { transform: "scale(1)" },
          ],
          { duration: 300, easing: "ease-out" },
        );
      last = { song: s?.id, marked };
    },
  };
}

function speedButton() {
  const b = h("button.icon-button.speed", {
    type: "button",
    title: "Speed",
    "aria-label": "Speed",
  });
  b.addEventListener("click", () => popover(b, speedPanel));
  return {
    el: b,
    update() {
      const sp = ctx.playback.speed;
      b.classList.toggle("on", sp !== 1);
      b.replaceChildren(
        sp !== 1 ? h("span.speedlabel", `${formatSpeed(sp)}×`) : icon("speed"),
      );
    },
  };
}

/** Speed (plan 016), for the song playing or as this browser's everyday one. Pitch is the apps' alone (plan 030). */
function speedPanel(close) {
  const p = ctx.playback;
  let only = !!p.songTempo;
  const title = h("h3");
  const range = h("input", {
    type: "range",
    min: minSpeed,
    max: maxSpeed,
    step: 0.05,
    "aria-label": "Speed",
  });
  const toggle = h("input", { type: "checkbox", checked: only, disabled: !p.current });
  const s = p.current;
  const what =
    s?.kind === "podcast" ? "episode" : s?.kind === "audiobook" ? "file" : "song";
  const note = h("p.note");
  const reset = h("button", { type: "button" }, "Reset");
  const draw = (v = p.speed) => {
    title.textContent = `Speed ${formatSpeed(v)}×`;
    range.value = v;
    note.textContent = only
      ? `This ${what} keeps its own speed, on all your devices.`
      : s?.spoken
        ? `For every ${s.kind === "podcast" ? "podcast" : "audiobook"} without its own, on all your devices.`
        : "For every song in this browser without its own.";
    reset.disabled = p.speed === 1;
  };
  const set = (v) => {
    p.setSpeed(Math.round(v * 20) / 20, only);
    draw();
  };
  range.addEventListener("input", () => draw(Number(range.value)));
  range.addEventListener("change", () => set(Number(range.value)));
  toggle.addEventListener("change", () => {
    only = toggle.checked;
    if (only) p.setSpeed(p.speed, true);
    else p.clearSongSpeed();
    draw();
  });
  reset.addEventListener("click", () => set(1));
  draw();
  const semis = p.songTempo?.semitones ?? 0;
  return h(
    "div.pop",
    title,
    range,
    h(
      "div.presets",
      [0.75, 1, 1.25, 1.5, 2].map((v) =>
        h("button", { type: "button", onclick: () => set(v) }, `${formatSpeed(v)}×`),
      ),
    ),
    h("label.check", toggle, `Only for this ${what}`),
    note,
    semis
      ? h(
          "p.note",
          `This song’s pitch (${semis > 0 ? "+" : "−"}${Math.abs(semis)}) is used by the phone and Mac apps; a browser keeps the pitch as it is.`,
        )
      : null,
    h(
      "div.panelbuttons",
      reset,
      h("button.primary", { type: "button", onclick: close }, "Done"),
    ),
  );
}

function sleepButton() {
  const b = iconButton("sleep", "Sleep timer", () => popover(b, sleepPanel));
  return {
    el: b,
    update() {
      const label = ctx.playback.sleepLabel;
      b.classList.toggle("on", !!label);
      b.title = label ? `Sleep ${label}` : "Sleep timer";
      b.setAttribute("aria-label", b.title);
    },
  };
}

/** The sleep timer (plan 014). */
function sleepPanel(close) {
  const p = ctx.playback;
  const done = (f) => () => {
    f();
    close();
  };
  const minutes = h("input", {
    type: "number",
    min: 1,
    max: 600,
    value: 20,
    "aria-label": "Minutes",
  });
  const songs = h("input", {
    type: "number",
    min: 2,
    max: 100,
    value: 3,
    "aria-label": "Songs",
  });
  return h(
    "div.pop.sleep",
    h("h3", p.sleepLabel ? `Stops ${p.sleepLabel}` : "Sleep timer"),
    h(
      "div.presets",
      [15, 30, 45, 60].map((m) =>
        h(
          "button",
          { type: "button", onclick: done(() => p.sleepMinutes(m)) },
          `${m} min`,
        ),
      ),
    ),
    h(
      "div.line",
      minutes,
      h("span", "minutes"),
      h(
        "button",
        {
          type: "button",
          onclick: done(() => p.sleepMinutes(Number(minutes.value) || 20)),
        },
        "Start",
      ),
    ),
    h("hr"),
    h(
      "button.plain",
      { type: "button", onclick: done(() => p.sleepEndOfSong()) },
      "At the end of this song",
    ),
    h(
      "div.line",
      h("span", "After"),
      songs,
      h("span", "songs"),
      h(
        "button",
        { type: "button", onclick: done(() => p.sleepSongs(Number(songs.value) || 3)) },
        "Start",
      ),
    ),
    h(
      "button.plain",
      { type: "button", onclick: done(() => p.sleepEndOfQueue()) },
      "At the end of the queue",
    ),
    p.sleep.mode
      ? [
          h("hr"),
          h(
            "button.plain.danger",
            { type: "button", onclick: done(() => p.cancelSleep()) },
            "Turn off",
          ),
        ]
      : null,
  );
}

/**
 * The song's title and the line under it, sliding in from the side the queue
 * moved to (plan 027). With `links`, each artist, the album, and an
 * episode's show or a book's title in that line open their pages (plan 032).
 */
function nowTitle(cls = "", links = false) {
  const box = h("div.nowtitle", { class: cls });
  // Nothing drawn yet: unlike any song id, null included.
  let shownId = box;
  let lastIndex = -1;
  const subLine = (s) => {
    const p = ctx.playback;
    if (!s) return [];
    if (p.waiting) return ["Waiting for the network…"];
    const parts = [...nowFacts(s), [[p.sleepLabel && `Sleep ${p.sleepLabel}`]]]
      .map((part) => part.filter(([text]) => text))
      .filter((part) => part.length);
    return parts.flatMap((part, i) => [
      i ? " · " : "",
      ...part.flatMap(([text, href], j) => [
        j ? ", " : "",
        links && href ? h("a", { href }, text) : text,
      ]),
    ]);
  };
  return {
    el: box,
    update() {
      const p = ctx.playback;
      const s = p.current;
      if (shownId === (s?.id ?? null)) {
        // Only when it changed: a link redrawn under the pointer loses its click.
        const sub = box.querySelector(".nowsub");
        const next = h("div.nowsub", ...subLine(s));
        if (next.innerHTML !== sub.innerHTML) sub.replaceWith(next);
        return;
      }
      const i = s ? p.items.indexOf(s.id) : -1;
      const forward =
        lastIndex < 0 || i > lastIndex || (lastIndex === p.items.length - 1 && i === 0);
      lastIndex = i;
      const inner = h(
        "div.nowinner",
        h("div.nowsong", s ? s.title : "Nothing playing"),
        h("div.nowsub", ...subLine(s)),
      );
      box.replaceChildren(inner);
      if (shownId !== box && !reducedMotion())
        inner.animate(
          [
            { transform: `translateX(${forward ? 24 : -24}px)`, opacity: 0 },
            { transform: "none", opacity: 1 },
          ],
          { duration: 280, easing: "cubic-bezier(.2,.8,.2,1)" },
        );
      shownId = s?.id ?? null;
    },
  };
}

/**
 * The parts of the line under the title, each a list of [text, page]: the
 * artists and the album as on the phone, or an episode's chapter, show and
 * day, or a book file's chapter, book and author.
 */
function nowFacts(s) {
  const c = ctx.app.catalog;
  const nav = ctx.nav;
  if (s.spoken) {
    const shelf = c.shelfOf(s);
    const [group, rest] = spokenFacts(s);
    return [
      [[chapterLabel(s, ctx.playback.position * 1000)]],
      [[group, shelf && nav.href("shelf", shelf.id)]],
      [[rest]],
    ];
  }
  const album = c.albumOf.get(s.id);
  return [
    // Each of several artists opens its own page, as Artists lists them apart.
    (s.artists.length ? s.artists : [s.displayArtist]).map((a) => [
      a,
      nav.href("artist", a),
    ]),
    [[s.album, album && nav.href("album", album.id)]],
  ];
}

/** The cover of the song playing; a new one fades in over the old. */
function nowCover(px, cls) {
  const box = h("div.nowcover", { class: cls });
  let id = box;
  return {
    el: box,
    update() {
      const s = ctx.playback.current;
      if ((s?.id ?? null) === id) return;
      id = s?.id ?? null;
      const c = cover(s, px);
      box.replaceChildren(c);
      if (!reducedMotion()) c.animate([{ opacity: 0.4 }, { opacity: 1 }], 280);
    },
  };
}

// MARK: The bar along the bottom of the window

export function nowBar() {
  const p = ctx.playback;
  const art = nowCover(128, "small");
  const title = nowTitle("", true);
  const t = transport();
  const seek = seeker();
  const fav = markButton("fav");
  const later = markButton("later");
  const speed = speedButton();
  const sleep = sleepButton();
  const lyrics = iconButton(
    "lyrics",
    "Lyrics",
    () => ctx.shell.togglePanel("lyrics"),
    "toggle",
  );
  const queue = iconButton(
    "queues",
    "Playing queue",
    () => ctx.shell.togglePanel("queue"),
    "toggle",
  );
  const pip =
    "documentPictureInPicture" in window
      ? iconButton("pip", "Mini player", () => ctx.shell.openMini())
      : null;
  const left = h("div.nowleft", art.el, title.el);
  left.addEventListener(
    "dblclick",
    (e) => p.current && !e.target.closest("a") && ctx.nav.album(p.current),
  );
  left.addEventListener("contextmenu", (e) => {
    if (!p.current) return;
    e.preventDefault();
    import("./kit.js").then(({ showMenu }) =>
      showMenu(songMenu([p.current]), { x: e.clientX, y: e.clientY }, p.current.title),
    );
  });
  const el = h(
    "footer.nowbar",
    left,
    h("div.nowcenter", t.el, seek.el),
    h("div.nowright", fav.el, later.el, speed.el, sleep.el, lyrics, queue, pip),
  );
  const parts = [art, title, t, seek, fav, later, speed, sleep];
  return {
    el,
    update() {
      for (const x of parts) x.update();
      const panel = ctx.shell.panel;
      lyrics.classList.toggle("on", panel === "lyrics");
      queue.classList.toggle("on", panel === "queue");
      lyrics.disabled = !p.current;
      lyricsTitle(lyrics);
    },
    position() {
      seek.update();
      // The chapter playing, under the title.
      if (p.current?.chapters.length) title.update();
    },
  };
}

// MARK: The phone's mini player and Now playing page

/** Art, title, play or pause and next; a tap opens Now playing (plan 011). */
export function miniPlayer() {
  const p = ctx.playback;
  const art = nowCover(128, "mini");
  const title = nowTitle();
  const play = h("button.icon-button", {
    type: "button",
    onclick: (e) => (e.stopPropagation(), p.toggle()),
  });
  const next = iconButton("next", "Next", (e) => (e.stopPropagation(), p.next(true)));
  const bar = h("div.miniprogress");
  const el = h(
    "div.mini",
    {
      role: "button",
      tabindex: "0",
      "aria-label": "Now playing",
      onclick: () => ctx.nav.go("now"),
    },
    bar,
    art.el,
    title.el,
    play,
    next,
  );
  return {
    el,
    update() {
      el.hidden = !p.current;
      art.update();
      title.update();
      play.replaceChildren(icon(p.isPlaying ? "pause" : "play"));
      play.title = p.isPlaying ? "Pause" : "Play";
      play.setAttribute("aria-label", play.title);
      this.position();
    },
    position() {
      bar.style.setProperty(
        "--fill",
        p.duration ? `${(p.position / p.duration) * 100}%` : "0",
      );
    },
  };
}

/** The phone's Now playing: the cover (or the lyrics over it), the song, the seek bar, the controls. */
export function nowPage() {
  const p = ctx.playback;
  const art = nowCover(1024, "large");
  const title = nowTitle("large", true);
  const t = transport();
  const seek = seeker();
  const fav = markButton("fav");
  const later = markButton("later");
  const speed = speedButton();
  const sleep = sleepButton();
  const lyr = lyricsView();
  let showLyrics = false;
  const stage = h("div.stage", art.el, lyr.el);
  const lyricsBtn = iconButton(
    "lyrics",
    "Lyrics",
    () => {
      showLyrics = !showLyrics;
      draw();
    },
    "toggle",
  );
  const queueBtn = iconButton(
    "queues",
    "Playing queue",
    () => p.activeId && ctx.nav.go("queue", p.activeId),
  );
  const moreBtn = iconButton(
    "more",
    "More",
    (e) => p.current && menuAt(e.currentTarget, songMenu([p.current]), p.current.title),
  );
  swipe(stage, (dir) => (dir < 0 ? p.next(true) : p.previous(true)));
  // The line under the title names the album, so the queue is named here, as on the phone.
  const queueName = h("div.nowqueue");
  const el = h(
    "section.page.nowpage",
    h(
      "header.pagebar",
      iconButton("down", "Close", () => ctx.nav.back(), "back"),
      h("span.spacer"),
      queueBtn,
      moreBtn,
    ),
    h(
      "div.nowbody",
      queueName,
      stage,
      title.el,
      seek.el,
      t.el,
      h("div.nowactions", fav.el, later.el, speed.el, sleep.el, lyricsBtn),
    ),
  );
  const parts = [art, title, t, seek, fav, later, speed, sleep];
  const draw = () => {
    stage.classList.toggle("lyrics", showLyrics);
    lyricsBtn.classList.toggle("on", showLyrics);
    lyr.update(showLyrics);
  };
  return {
    el,
    title: "Now playing",
    topics: new Set(["playback", "position", "marks", "settings", "thumbs"]),
    destroy: () => lyr.destroy(),
    update(topics) {
      if (topics.has("position") && topics.size === 1) {
        seek.update();
        lyr.position();
        return;
      }
      for (const x of parts) x.update();
      queueName.textContent = p.active?.name ?? "";
      queueBtn.disabled = !p.activeId;
      moreBtn.disabled = !p.current;
      lyricsBtn.disabled = !p.current;
      lyricsTitle(lyricsBtn);
      draw();
    },
  };
}

/** An episode's or a book's lyrics are its transcript (plan 031). */
function lyricsTitle(b) {
  b.title = ctx.playback.current?.spoken ? "Transcript" : "Lyrics";
  b.setAttribute("aria-label", b.title);
}

/** Horizontal swipes on `el`: −1 for left (next), 1 for right (previous). */
function swipe(el, f) {
  let x0 = null;
  let y0 = 0;
  el.addEventListener("pointerdown", (e) => {
    if (e.pointerType === "mouse") return;
    x0 = e.clientX;
    y0 = e.clientY;
  });
  el.addEventListener("pointerup", (e) => {
    if (x0 == null) return;
    const dx = e.clientX - x0;
    const dy = e.clientY - y0;
    x0 = null;
    if (Math.abs(dx) > 60 && Math.abs(dx) > 2 * Math.abs(dy)) f(dx < 0 ? -1 : 1);
  });
  el.addEventListener("pointercancel", () => (x0 = null));
}

// MARK: Banners above Now playing

/** Hand-off, "continue from…?", folded in and out (plan 017). */
export function banners() {
  const el = h("div.banners");
  let key = "";
  return {
    el,
    update() {
      const p = ctx.playback;
      // A note from the player (a queue removed to make room, the server gone) shows once, as a toast.
      if (p.notice) {
        toast(p.notice);
        p.notice = null;
      }
      const np = p.handoff;
      const s = np && ctx.app.catalog.byId.get(np.song);
      const o = p.offerResume;
      const next = `${np?.at}|${s?.id}|${o?.song.id}`;
      if (next === key) return;
      key = next;
      const items = [];
      if (s) {
        const from = np.deviceName || "another device";
        items.push(
          banner(
            "handoff",
            [
              `Continue from ${from} — `,
              h("b", s.title),
              `, ${clock(p.placeOf(np) / 1000)}`,
            ],
            [
              h(
                "button.primary",
                { type: "button", onclick: () => p.continueFrom(np) },
                "Continue",
              ),
              iconButton("close", "Dismiss", () => p.dismissHandoff(np)),
            ],
          ),
        );
      }
      if (o) {
        items.push(
          banner(
            "continue",
            [`Continue “${o.song.title}” from ${clock(o.ms / 1000)}?`],
            [
              h(
                "button.primary",
                { type: "button", onclick: () => p.answerResume(true) },
                "Continue",
              ),
              h(
                "button",
                { type: "button", onclick: () => p.answerResume(false) },
                "Start over",
              ),
            ],
          ),
        );
      }
      el.replaceChildren(...items);
    },
  };
}

function banner(name, message, actions) {
  return h(
    "div.banner",
    icon(name),
    h("span.bannertext", message),
    h("span.banneractions", actions),
  );
}

// MARK: Lyrics (plan 015)

/**
 * Synced lines follow the song and a click on one plays from it; plain
 * lyrics are just text. A scroll by hand holds the lines where they are for a
 * few seconds, as on the phone. While shown and playing, the screen stays on
 * where the browser allows it.
 */
export function lyricsView() {
  const el = h("div.lyrics");
  let song = null;
  let parsed = null;
  let lines = [];
  let at = -2;
  let heldUntil = 0;
  let visible = true;
  let lock = null;
  el.addEventListener("wheel", () => (heldUntil = Date.now() + 4000), {
    passive: true,
  });
  el.addEventListener("touchmove", () => (heldUntil = Date.now() + 4000), {
    passive: true,
  });
  const note = (title, name = "lyrics") =>
    h("div.empty", icon(name, "big"), h("p.empty-title", title));
  const load = async (s) => {
    parsed = null;
    lines = [];
    at = -2;
    if (!s) return el.replaceChildren(note("Nothing playing"));
    // An episode's or a book's lyrics are its transcript (plan 031).
    const none = s.spoken ? "No transcript" : "No lyrics";
    if (!s.hasLyrics) return el.replaceChildren(note(none));
    el.replaceChildren(h("div.empty", h("div.spinner")));
    try {
      const text = await ctx.app.lyrics(s.id);
      if (song?.id !== s.id) return;
      if (text == null) return el.replaceChildren(note(none));
      parsed = parseLyrics(text);
      lines = parsed.lines.map((l) =>
        h(
          "p.line",
          {
            class: parsed.synced ? "synced" : "",
            onclick: parsed.synced ? () => ctx.playback.seek(l.ms / 1000) : null,
          },
          l.text || " ",
        ),
      );
      el.replaceChildren(
        h("div.lines", { class: parsed.synced ? "synced" : "" }, lines),
      );
      el.scrollTop = 0;
      position();
    } catch {
      if (song?.id === s.id)
        el.replaceChildren(note("Lyrics need the server", "offline"));
    }
  };
  const position = () => {
    if (!parsed?.synced || !visible) return;
    const i = lineAt(parsed, ctx.playback.position * 1000);
    if (i === at) return;
    lines[at]?.classList.remove("now");
    lines[i]?.classList.add("now");
    at = i;
    if (Date.now() >= heldUntil && lines[Math.max(0, i)]) {
      const l = lines[Math.max(0, i)];
      el.scrollTo({
        top: l.offsetTop - el.clientHeight / 2 + l.clientHeight / 2,
        behavior: reducedMotion() ? "auto" : "smooth",
      });
    }
  };
  const awake = async (on) => {
    if (on && !lock && navigator.wakeLock) {
      lock = await navigator.wakeLock.request("screen").catch(() => null);
      // The browser lets go when the page is hidden; it is asked again on the next update.
      lock?.addEventListener("release", () => (lock = null));
    } else if (!on && lock) {
      lock.release();
      lock = null;
    }
  };
  return {
    el,
    update(show = true) {
      visible = show;
      const s = ctx.playback.current;
      if (show && s?.id !== song?.id) {
        song = s;
        load(s);
      }
      awake(show && ctx.playback.isPlaying && !!parsed);
      position();
    },
    position,
    destroy: () => awake(false),
  };
}
