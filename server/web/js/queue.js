// The pure parts of playing, kept apart from the player so Node's tests can
// load them: the queues' order, the play order, and speed.

/** At most this many queues, Musicolet's limit (plan 020). */
export const maxQueues = 20;
/** The synced setting with the queues' order: their ids, as a JSON array. */
export const orderSetting = "queues.order";

/**
 * The queues in the user's order, numbered from 1 as in Musicolet (plan 020).
 * The order is a synced setting: it can name queues deleted elsewhere, miss
 * ones made on another device, or come from a newer app in another shape.
 * Named first, in order; the rest by use, oldest first, as Musicolet appends.
 */
export function orderOf(queues, order) {
  const ids = Array.isArray(order) ? order.filter((x) => typeof x === "string") : [];
  const byId = new Map(queues.map((q) => [q.id, q]));
  const seen = new Set();
  const named = [];
  for (const id of ids) {
    if (seen.has(id) || !byId.has(id)) continue;
    seen.add(id);
    named.push(byId.get(id));
  }
  const rest = queues
    .filter((q) => !seen.has(q.id))
    .sort((a, b) => a.usedAt - b.usedAt);
  return [...named, ...rest];
}

/**
 * The play order of `items`. Shuffled, a queue keeps its play order across
 * edits: songs keep their places, new ones go in at random after the playing
 * one. Shuffling afresh puts the playing song first, as Media3 does.
 */
export function playOrder(items, previous, current, shuffle, keep) {
  if (!shuffle) return items.slice();
  if (keep && previous.length) {
    const present = new Set(items);
    const kept = previous.filter((id) => present.has(id));
    const have = new Set(kept);
    const at = current == null ? -1 : kept.indexOf(current);
    for (const id of items) {
      if (have.has(id)) continue;
      const lo = at + 1;
      kept.splice(lo + Math.floor(Math.random() * (kept.length - lo + 1)), 0, id);
    }
    return kept;
  }
  const rest = items.filter((id) => id !== current);
  for (let i = rest.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [rest[i], rest[j]] = [rest[j], rest[i]];
  }
  if (current != null && items.includes(current)) rest.unshift(current);
  return rest;
}

/** The song after `song` in `order`, under `repeat` (off, queue, song); null at the end. */
export function nextIn(order, song, repeat) {
  if (repeat === "song") return song;
  const at = order.indexOf(song);
  if (at < 0) return null;
  if (at + 1 < order.length) return order[at + 1];
  return repeat === "queue" ? order[0] : null;
}

/**
 * Play speed (plan 016). The phone and the Mac shift the pitch too; a
 * browser keeps the pitch while changing speed, and cannot shift it alone
 * without a pitch shifter of our own (docs/plans/030_web_client.md), so a
 * song's semitones are kept in its setting and left unused here.
 */
export const minSpeed = 0.5;
export const maxSpeed = 2;
export const maxSemitones = 6;

/** A song's setting (`speed.<id>`), or null when it has none or it cannot be read. */
export function tempoOf(v) {
  if (v == null || typeof v !== "object" || Array.isArray(v)) return null;
  const speed = typeof v.speed === "number" ? v.speed : 1;
  const semitones = Number.isInteger(v.semitones) ? v.semitones : 0;
  return {
    speed: Math.min(Math.max(speed, minSpeed), maxSpeed),
    semitones: Math.min(Math.max(semitones, -maxSemitones), maxSemitones),
  };
}

export const songSetting = (song) => `speed.${song}`;

/** 1.25 → "1.25", 1.5 → "1.5", 2 → "2", in every locale. */
export function formatSpeed(speed) {
  const hundredths = Math.round(speed * 100);
  if (hundredths % 100 === 0) return String(hundredths / 100);
  return `${Math.floor(hundredths / 100)}.${String(hundredths % 100).padStart(2, "0")}`.replace(
    /0+$/,
    "",
  );
}
