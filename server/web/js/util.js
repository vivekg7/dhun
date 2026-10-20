// Small helpers every module uses: building DOM, times, and formatting.

/**
 * An element: `h("div.row.on", {onclick}, child, "text", [more])`. Props
 * starting with "on" are listeners; `class`, `style` and `dataset` are set
 * as such; false, null and undefined children are skipped.
 */
export function h(tag, props, ...children) {
  const [name, ...classes] = tag.split(".");
  const el =
    name === "svg" || name === "path"
      ? document.createElementNS(svgNS, name)
      : document.createElement(name || "div");
  if (classes.length) el.classList.add(...classes);
  if (
    props &&
    (typeof props !== "object" || props instanceof Node || Array.isArray(props))
  ) {
    children.unshift(props);
    props = null;
  }
  for (const [k, v] of Object.entries(props ?? {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.classList.add(...String(v).split(" ").filter(Boolean));
    else if (k === "style" && typeof v === "object")
      for (const [name, value] of Object.entries(v))
        el.style.setProperty(
          name.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase()),
          value,
        );
    else if (k === "dataset") Object.assign(el.dataset, v);
    else if (k in el && !(el instanceof SVGElement) && typeof v !== "string") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  append(el, children);
  return el;
}

function append(el, children) {
  for (const c of children) {
    if (c == null || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else el.append(c instanceof Node ? c : String(c));
  }
}

const svgNS = "http://www.w3.org/2000/svg";

/** "3:07", "1:02:45" */
export function clock(seconds) {
  const t = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hh = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = String(t % 60).padStart(2, "0");
  return hh > 0 ? `${hh}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** "1 song", "12 songs" */
export const count = (n, one = "song", many = one + "s") =>
  `${n} ${n === 1 ? one : many}`;

/** "12 songs · 48:10" */
export const summary = (songs) =>
  `${count(songs.length)} · ${clock(songs.reduce((a, s) => a + s.durationMs, 0) / 1000)}`;

export const nowMs = () => Date.now();
export const parseTime = (s) => (s ? Date.parse(s) || 0 : 0);
export const formatTime = (ms) => new Date(ms).toISOString();
/**
 * A random (v4) UUID. Not crypto.randomUUID: it exists only in a secure
 * context, and the NAS is usually reached over plain http on the LAN or
 * Tailscale. getRandomValues works everywhere.
 */
export function uuid() {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const x = [...b].map((v) => v.toString(16).padStart(2, "0")).join("");
  return `${x.slice(0, 8)}-${x.slice(8, 12)}-${x.slice(12, 16)}-${x.slice(16, 20)}-${x.slice(20)}`;
}

/**
 * The key whose plain string order is natural, case-blind order: "Track 2"
 * before "Track 10". Each run of digits is padded to twelve places, so one
 * is read as a number ("0" and "00" stay one digit). Computed once per item
 * and compared with `<`, where a comparator that split both strings on
 * every call took half the catalogue build at 100,000 songs (plan 036).
 */
export const naturalKey = (s) =>
  s
    .toLowerCase()
    .replace(/\p{Nd}+/gu, (d) => d.replace(/^0+(?=\p{Nd})/u, "").padStart(12, "0"));

const compare = (x, y) => (x < y ? -1 : x > y ? 1 : 0);

/**
 * `list` sorted by what `key` gives each item: a string or number, or an
 * array of them compared in order. Each key is computed once, not once per
 * comparison, which is what keeps a sort of 100,000 songs cheap.
 */
export function sortBy(list, key) {
  return list
    .map((item) => [key(item), item])
    .sort(([a], [b]) => {
      if (!Array.isArray(a)) return compare(a, b);
      for (let i = 0; i < a.length; i++) {
        const c = compare(a[i], b[i]);
        if (c) return c;
      }
      return 0;
    })
    .map((x) => x[1]);
}

/** The locale's case-blind order, as the apps' sorts use. */
const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
export const compareText = (a, b) => collator.compare(a, b);

export const distinct = (ids) => [...new Set(ids)];

export function shuffled(list) {
  const a = list.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

/** Calls `f` once, `ms` after the last call. */
export function debounce(f, ms) {
  let t;
  return (...args) => {
    clearTimeout(t);
    t = setTimeout(() => f(...args), ms);
  };
}

/** "3 hours ago", "just now" */
export function ago(ms) {
  const s = (Date.now() - ms) / 1000;
  if (s < 60) return "just now";
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [unit, n] of [
    ["year", 31536000],
    ["month", 2592000],
    ["week", 604800],
    ["day", 86400],
    ["hour", 3600],
    ["minute", 60],
  ])
    if (s >= n) return rtf.format(-Math.floor(s / n), unit);
  return "just now";
}
