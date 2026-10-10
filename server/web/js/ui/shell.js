// The window around the pages (docs/plans/030_web_client.md): at 900 px and
// wider, the Mac's sidebar, the page, the queue or lyrics panel and the Now
// playing bar; narrower, the phone's icon tabs with a mini player above them.
// Every page has an address (#/album/…), so Back, a reload and a bookmark
// all work.

import { h } from "../util.js";
import { icon } from "../icons.js";
import { lists } from "../catalog.js";
import { iconButton, closeMenu, touch, reducedMotion } from "./kit.js";
import { addedTo, editable, cover } from "./songs.js";
import * as pages from "./pages.js";
import { searchPage } from "./searchpage.js";
import { settingsPage, signInPage } from "./settings.js";
import { nowBar, miniPlayer, nowPage, banners, lyricsView } from "./player.js";
import { ctx } from "./ctx.js";

const wideQuery = matchMedia("(min-width: 900px)");

/** Which sidebar item or tab a page belongs to. */
const sectionOf = {
  queue: "queues",
  album: "albums",
  artist: "artists",
  genre: "genres",
  folder: "folders",
  playlist: "playlists",
};

export class Nav {
  constructor(onChange) {
    this.onChange = onChange;
    this.route = this.parse();
    addEventListener("popstate", () => this.changed());
    addEventListener("hashchange", () => this.changed());
    // Links go through history.pushState, so each page knows how deep it is and Back can stay in the app.
    document.addEventListener("click", (e) => {
      const a = e.target.closest?.('a[href^="#/"]');
      if (
        !a ||
        e.defaultPrevented ||
        e.button !== 0 ||
        e.metaKey ||
        e.ctrlKey ||
        e.shiftKey
      )
        return;
      e.preventDefault();
      this.push(a.getAttribute("href"));
    });
  }

  parse() {
    const [, name = "", ...rest] = location.hash.split("/");
    const arg = rest.length ? decodeURIComponent(rest.join("/")) : null;
    return { name: name || "albums", arg };
  }

  href(name, arg) {
    return `#/${name}${arg == null ? "" : `/${encodeURIComponent(arg)}`}`;
  }

  go(name, arg) {
    this.push(this.href(name, arg));
  }

  push(href) {
    if (href === location.hash) return;
    this.saveScroll();
    history.pushState({ depth: (history.state?.depth ?? 0) + 1 }, "", href);
    this.changed();
  }

  replace(name, arg) {
    history.replaceState({ ...history.state }, "", this.href(name, arg));
    this.changed();
  }

  /** Back within the app, or up to the section when the page was opened directly. */
  back() {
    if ((history.state?.depth ?? 0) > 0) history.back();
    else this.replace(this.section());
  }

  section(route = this.route) {
    return sectionOf[route.name] ?? route.name;
  }

  album(s) {
    const a = ctx.app.catalog.albumOf.get(s.id);
    if (a) this.go("album", a.id);
  }

  artist(s) {
    this.go("artist", s.artists[0] ?? s.displayArtist);
  }

  saveScroll() {
    const el = ctx.shell?.scroller();
    if (el) history.replaceState({ ...history.state, scroll: el.scrollTop }, "");
  }

  changed() {
    const before = this.route;
    this.route = this.parse();
    this.onChange(before, this.route);
  }
}

export class Shell {
  constructor(root) {
    this.root = root;
    this.nav = new Nav((from, to) => this.routeChanged(from, to));
    // Set before the first build: the views it makes reach for both.
    ctx.shell = this;
    ctx.nav = this.nav;
    this.panel = ctx.app.prefs.panel ?? null;
    this.pending = new Set();
    const onChange = (topic) => {
      this.pending.add(topic);
      if (!this.frame) this.frame = requestAnimationFrame(() => this.flush());
    };
    ctx.app.addEventListener("change", (e) => {
      if (e.detail === "account") return this.accountChanged();
      onChange(e.detail);
    });
    ctx.playback.addEventListener("state", () => onChange("playback"));
    ctx.playback.addEventListener("position", () => onChange("position"));
    wideQuery.addEventListener("change", () => this.build());
    addEventListener("keydown", (e) => this.key(e));
    this.accountChanged();
  }

  accountChanged() {
    if (this.signedIn === ctx.app.signedIn && this.built) return;
    this.signedIn = ctx.app.signedIn;
    if (!this.signedIn) {
      this.built = false;
      this.page?.destroy?.();
      this.page = null;
      this.root.replaceChildren(signInPage(() => this.accountChanged()));
      document.title = "Dhun";
      return;
    }
    this.build();
  }

  // MARK: Layout

  build() {
    if (!ctx.app.signedIn) return;
    this.built = true;
    this.wide = wideQuery.matches;
    this.host = h("main.host");
    this.banners = banners();
    if (this.wide) {
      this.search = h("input.searchinput", {
        type: "search",
        placeholder: "Songs, albums, artists, lyrics",
        "aria-label": "Search",
        autocomplete: "off",
        spellcheck: false,
      });
      this.search.addEventListener("input", () => this.searched());
      this.search.addEventListener("keydown", (e) => {
        if (e.key === "Escape") {
          this.search.value = "";
          this.searched();
          this.search.blur();
        } else if (e.key === "ArrowDown") this.page?.focus?.();
      });
      this.sidebar = h("nav.sidebar", { "aria-label": "Library" });
      this.panelEl = h("aside.panel", { "aria-label": "Queue and lyrics" });
      this.bar = nowBar();
      this.root.replaceChildren(
        h(
          "div.app.wide",
          this.sidebar,
          h(
            "div.center",
            h(
              "div.topbar",
              iconButton("back", "Back", () => history.back(), "nav"),
              iconButton("right", "Forward", () => history.forward(), "nav"),
              h("label.searchfield", icon("search"), this.search),
              (this.badge ??= syncBadge()),
            ),
            this.host,
          ),
          this.panelEl,
          h("div.bottom", this.banners.el, this.bar.el),
        ),
      );
    } else {
      this.mini = miniPlayer();
      this.tabs = h("nav.tabs", { "aria-label": "Sections" });
      this.root.replaceChildren(
        h(
          "div.app.narrow",
          this.host,
          h("div.bottom", this.banners.el, this.mini.el, this.tabs),
        ),
      );
      this.dropPanel();
    }
    this.page?.destroy?.();
    this.page = null;
    this.routeChanged(null, this.nav.route);
    this.flush(new Set(["all"]));
  }

  /** The window's search field: typing shows the results, over any page opened from them. */
  searched() {
    const q = this.search.value;
    if (this.nav.route.name === "search") this.nav.replace("search", q);
    else if (q) this.nav.go("search", q);
  }

  routeChanged(from, to) {
    if (!this.built) return;
    closeMenu();
    // On a wide window Now playing is the bar, not a page.
    if (this.wide && to.name === "now") return this.nav.replace("queues");
    if (
      this.wide &&
      to.name === "search" &&
      this.page?.setQuery &&
      from?.name === "search"
    ) {
      this.page.setQuery(to.arg ?? "");
      return;
    }
    const page = this.makePage(to);
    // Drawn whole once; after that, only what changes.
    page.update(new Set(page.topics));
    const old = this.page;
    old?.destroy?.();
    this.page = page;
    page.el.classList.add("entering");
    this.host.replaceChildren(page.el);
    // Pages slide a fifth of the width, from the side they open on (plan 027).
    const deeper = (history.state?.depth ?? 0) >= (this.lastDepth ?? 0);
    this.lastDepth = history.state?.depth ?? 0;
    if (old && from && !reducedMotion()) {
      page.el.animate(
        [
          { transform: `translateX(${deeper ? 20 : -20}%)`, opacity: 0 },
          { transform: "none", opacity: 1 },
        ],
        { duration: 280, easing: "cubic-bezier(.2,.8,.2,1)" },
      );
    }
    requestAnimationFrame(() => {
      page.el.classList.remove("entering");
      const s = history.state?.scroll;
      const el = this.scroller();
      if (el && s != null) {
        el.scrollTop = s;
        el.dispatchEvent(new Event("scroll"));
      }
    });
    if (this.wide) {
      const q = to.name === "search" ? (to.arg ?? "") : "";
      if (this.search.value !== q && document.activeElement !== this.search)
        this.search.value = q;
      if (to.name !== "search" && document.activeElement === this.search)
        this.search.value = "";
      if (!touch() && to.name !== "search") page.focus?.();
    }
    this.drawNav();
    this.drawTitle();
  }

  /** The page made again for the same address: what it shows may have arrived. */
  reopen() {
    if (this.built) this.routeChanged(null, this.nav.route);
  }

  makePage({ name, arg }) {
    switch (name) {
      case "queues":
        return pages.queuesPage();
      case "queue":
        return pages.queuePage(arg);
      case "now":
        return nowPage();
      case "folders":
        return pages.folderPage("");
      case "folder":
        return pages.folderPage(arg ?? "");
      case "albums":
        return pages.albumsPage();
      case "album":
        return pages.albumPage(arg);
      case "artists":
        return pages.groupsPage("artist");
      case "artist":
        return pages.artistPage(arg);
      case "genres":
        return pages.groupsPage("genre");
      case "genre":
        return pages.genrePage(arg);
      case "playlists":
        return pages.playlistsPage();
      case "playlist":
        return pages.playlistPage(Number(arg));
      case "list":
        return pages.listPage(arg);
      case "search":
        return searchPage(arg ?? "", { field: !this.wide });
      case "settings":
        return settingsPage();
      default:
        return pages.albumsPage();
    }
  }

  /** The page's own scroller, for keeping its place across Back. */
  scroller() {
    return this.page?.scroller ?? this.page?.el.querySelector(".vlist, .scroll");
  }

  // MARK: Updates

  flush(force) {
    this.frame = 0;
    const t = force ?? this.pending;
    this.pending = new Set();
    if (!this.built) return;
    const all = t.has("all");
    const has = (...xs) => all || xs.some((x) => t.has(x));
    if (this.page && (all || [...t].some((x) => this.page.topics?.has(x)))) {
      const mine = all
        ? this.page.topics
        : new Set([...t].filter((x) => this.page.topics.has(x)));
      this.page.update(mine);
    }
    if (has("playback", "marks", "settings", "thumbs", "catalog")) {
      this.bar?.update();
      this.mini?.update();
      this.drawTitle();
    } else if (t.has("position")) {
      this.bar?.position();
      this.mini?.position();
    }
    if (has("playback", "nowPlaying", "queues", "catalog")) this.banners.update();
    if (has("playlists", "queues", "sync", "marks", "playback", "catalog"))
      this.drawNav();
    if (this.wide) this.drawPanel(t);
  }

  drawTitle() {
    const s = ctx.playback.current;
    document.title =
      s && ctx.playback.isPlaying
        ? `${s.title} · ${s.displayArtist}`
        : this.page?.title
          ? `${this.page.title} · Dhun`
          : "Dhun";
  }

  // MARK: The sidebar and the tabs

  drawNav() {
    const r = this.nav.route;
    const here = (name, arg) =>
      (r.name === name && (arg === undefined || r.arg === String(arg))) ||
      (arg === undefined && this.nav.section(r) === name);
    if (!this.wide) {
      const tabs = [
        ["queues", "Queues", "queues"],
        ["now", "Now playing", "nowPlaying"],
        ["folders", "Folders", "folder"],
        ["albums", "Albums", "album"],
        ["artists", "Artists", "artist"],
        ["genres", "Genres", "genre"],
        ["playlists", "Playlists", "playlist"],
        ["search", "Search", "search"],
        ["settings", "Settings", "settings"],
      ];
      const sec = r.name === "list" ? "playlists" : this.nav.section(r);
      this.tabs.replaceChildren(
        ...tabs.map(([name, label, ic]) =>
          h(
            "a.tab",
            {
              href: this.nav.href(name),
              title: label,
              "aria-label": label,
              "aria-current": sec === name ? "page" : null,
              class: sec === name ? "on" : "",
            },
            icon(ic),
          ),
        ),
      );
      this.mini.el.classList.toggle("away", r.name === "now");
      return;
    }
    const app = ctx.app;
    const item = (name, arg, label, ic, extra = {}) =>
      h(
        "a.navitem",
        {
          href: this.nav.href(name, arg),
          "aria-current": here(name, arg) ? "page" : null,
          class: here(name, arg) ? "on" : "",
          ...extra,
        },
        icon(ic),
        h("span", label),
      );
    const dropTo = (add) => ({
      ondragover: (e) => {
        if (!e.dataTransfer.types.includes("application/x-dhun-songs")) return;
        e.preventDefault();
        e.currentTarget.classList.add("drop");
      },
      ondragleave: (e) => e.currentTarget.classList.remove("drop"),
      ondrop: (e) => {
        e.preventDefault();
        e.currentTarget.classList.remove("drop");
        const ids = JSON.parse(
          e.dataTransfer.getData("application/x-dhun-songs") || "[]",
        );
        add(app.catalog.songsOf(ids));
      },
    });
    const active = ctx.playback.active;
    this.sidebar.replaceChildren(
      h(
        "div.navscroll",
        item("queues", undefined, active ? `Queues` : "Queues", "queues"),
        h("h2.navhead", "Library"),
        item("folders", undefined, "Folders", "folder"),
        item("albums", undefined, "Albums", "album"),
        item("artists", undefined, "Artists", "artist"),
        item("genres", undefined, "Genres", "genre"),
        h("h2.navhead", "Lists"),
        ...lists.map((l) =>
          item(
            "list",
            l.id,
            l.title,
            l.icon,
            l.mark
              ? dropTo((songs) => songs.forEach((s) => app.mark(l.mark, s.id, true)))
              : {},
          ),
        ),
        h("h2.navhead", "Playlists"),
        ...app.playlists.map((p) =>
          item(
            "playlist",
            p.id,
            p.name,
            p.shared ? "shared" : "playlist",
            editable(p) ? dropTo((songs) => addedTo(p, songs)) : {},
          ),
        ),
      ),
      h(
        "div.navfoot",
        h(
          "button.newplaylist",
          { type: "button", onclick: pages.newPlaylist },
          icon("add"),
          "New playlist",
        ),
        h("span.spacer"),
        h(
          "a.icon-button",
          {
            href: this.nav.href("settings"),
            title: "Settings",
            "aria-label": "Settings",
            class: r.name === "settings" ? "on" : "",
          },
          icon("settings"),
        ),
      ),
    );
  }

  // MARK: The right-hand panel: the playing queue or the lyrics

  /** The panel's view goes: what it holds (a screen kept awake for lyrics) is let go. */
  dropPanel() {
    this.panelView?.view.destroy?.();
    this.panelView = null;
  }

  togglePanel(kind) {
    this.panel = this.panel === kind ? null : kind;
    ctx.app.setPref("panel", this.panel ?? undefined);
    this.dropPanel();
    this.drawPanel(new Set(["all"]));
    this.bar.update();
  }

  drawPanel(t) {
    const kind = this.panel;
    this.root.firstChild?.classList.toggle("with-panel", !!kind);
    if (!kind) {
      this.panelEl.replaceChildren();
      this.dropPanel();
      return;
    }
    // The queue's view is made again when another queue plays, or when the queues first arrive.
    const want =
      kind === "queue"
        ? `queue:${ctx.playback.activeId}:${!!ctx.playback.active}`
        : "lyrics";
    if (this.panelView?.key !== want) {
      const tabs = h(
        "div.segmented",
        h(
          "button",
          {
            type: "button",
            class: kind === "queue" ? "on" : "",
            onclick: () => kind !== "queue" && this.togglePanel("queue"),
          },
          "Queue",
        ),
        h(
          "button",
          {
            type: "button",
            class: kind === "lyrics" ? "on" : "",
            onclick: () => kind !== "lyrics" && this.togglePanel("lyrics"),
          },
          "Lyrics",
        ),
      );
      let view;
      if (kind === "lyrics") {
        const l = lyricsView();
        view = {
          el: l.el,
          update: (topics) =>
            topics.has("position") && topics.size === 1 ? l.position() : l.update(true),
          topics: new Set(["playback", "position"]),
          destroy: () => l.destroy(),
        };
      } else if (ctx.playback.active) {
        view = pages.queuePage(ctx.playback.activeId, { compact: true });
      } else {
        view = {
          el: h(
            "div.empty",
            icon("queues", "big"),
            h("p.empty-title", "Nothing playing"),
          ),
          update() {},
          topics: new Set(),
        };
      }
      this.dropPanel();
      this.panelView = { key: want, view };
      this.panelEl.replaceChildren(tabs, view.el);
      view.update(new Set(view.topics));
      return;
    }
    const v = this.panelView.view;
    const all = t.has("all");
    const mine = new Set([...(all ? v.topics : t)].filter((x) => v.topics.has(x)));
    if (mine.size) v.update(mine);
  }

  // MARK: Keys

  key(e) {
    if (!this.built || e.defaultPrevented) return;
    const typing =
      e.target.closest?.("input, textarea, select, [contenteditable]") &&
      e.target.type !== "range" &&
      e.target.type !== "checkbox";
    const mod = e.metaKey || e.ctrlKey;
    if (document.querySelector("dialog[open], .menu-backdrop")) return;
    if (e.key === " " && !typing && !mod) {
      e.preventDefault();
      ctx.playback.toggle();
    } else if (
      mod &&
      !e.shiftKey &&
      !e.altKey &&
      (e.key === "ArrowLeft" || e.key === "ArrowRight") &&
      !typing
    ) {
      e.preventDefault();
      e.key === "ArrowLeft" ? ctx.playback.previous() : ctx.playback.next();
    } else if (
      (e.key === "/" && !typing && !mod) ||
      (mod && e.key.toLowerCase() === "k")
    ) {
      e.preventDefault();
      if (this.wide) {
        this.search.focus();
        this.search.select();
      } else
        this.nav.go(
          "search",
          this.nav.route.name === "search" ? (this.nav.route.arg ?? "") : "",
        );
    }
  }

  // MARK: The mini player in its own window (Document Picture-in-Picture)

  /** A small window that stays on top of the others, as the Mac's floating mini window. */
  async openMini() {
    if (this.pip) return this.pip.focus();
    const w = await documentPictureInPicture.requestWindow({ width: 340, height: 132 });
    this.pip = w;
    for (const link of document.querySelectorAll('link[rel="stylesheet"]'))
      w.document.head.append(link.cloneNode());
    w.document.documentElement.dataset.theme = document.documentElement.dataset.theme;
    w.document.documentElement.style.cssText = document.documentElement.style.cssText;
    const p = ctx.playback;
    const art = h("div.pipcover");
    const title = h("div.pipsong");
    const sub = h("div.pipsub");
    const play = h("button.play", { type: "button", onclick: () => p.toggle() });
    const bar = h("div.miniprogress");
    w.document.body.className = "pip";
    w.document.body.append(
      h(
        "div.pipbox",
        art,
        h(
          "div.pipinfo",
          title,
          sub,
          h(
            "div.transport",
            iconButton("previous", "Previous", () => p.previous()),
            play,
            iconButton("next", "Next", () => p.next()),
          ),
        ),
      ),
      bar,
    );
    let song;
    const draw = () => {
      const s = p.current;
      if (s?.id !== song) {
        song = s?.id;
        art.replaceChildren(cover(s, 256));
      }
      title.textContent = s?.title ?? "Nothing playing";
      sub.textContent = s?.displayArtist ?? "";
      play.replaceChildren(icon(p.isPlaying ? "pause" : "play"));
      play.title = p.isPlaying ? "Pause" : "Play";
    };
    const pos = () =>
      bar.style.setProperty(
        "--fill",
        p.duration ? `${(p.position / p.duration) * 100}%` : "0",
      );
    p.addEventListener("state", draw);
    p.addEventListener("position", pos);
    draw();
    w.addEventListener("pagehide", () => {
      p.removeEventListener("state", draw);
      p.removeEventListener("position", pos);
      this.pip = null;
    });
  }
}

/** Offline or syncing, small at the top right. */
function syncBadge() {
  const el = h("span.syncbadge");
  const draw = () => {
    const app = ctx.app;
    el.replaceChildren(
      !app.reachable
        ? h(
            "span.offline",
            {
              title:
                "The server cannot be reached. Changes are kept and sent when it is back.",
            },
            icon("offline"),
            "Offline",
          )
        : app.syncing
          ? h("span.spinner.small", { title: "Syncing" })
          : app.syncError
            ? h("span.warn", { title: app.syncError }, icon("warning"))
            : "",
    );
  };
  ctx.app.addEventListener("change", (e) => e.detail === "sync" && draw());
  draw();
  return el;
}
