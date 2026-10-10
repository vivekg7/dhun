// Lyrics (plan 015), as the server stores them, by the phone's and the Mac's
// small LRC parser: `[mm:ss.xx]` stamps, several on one line for a repeated
// chorus, `[offset:±ms]`, and word stamps (`<mm:ss.xx>`, enhanced LRC), which
// are dropped. Text with no stamps is plain lyrics, without its `[ar:…]` tags.

const stamp = /^\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/;
const tagLine = /^\[([a-zA-Z#]+):(.*)\]$/;
const wordStamp = /<\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?>/g;

/** `{synced, lines: [{ms, text}]}`; `ms` is -1 in lyrics without times. */
export function parseLyrics(text) {
  let offset = 0;
  const timed = [];
  const plain = [];
  for (const raw of text.split(/\r\n|\r|\n/)) {
    let line = raw.trim();
    const tag = line.match(tagLine);
    if (tag && !new RegExp(stamp.source + "$").test(line)) {
      if (tag[1].toLowerCase() === "offset") offset = parseInt(tag[2].trim(), 10) || 0;
      continue;
    }
    const times = [];
    let m;
    while ((m = line.match(stamp))) {
      const frac = (m[3] ?? "").padEnd(3, "0");
      times.push(Number(m[1]) * 60_000 + Number(m[2]) * 1000 + (Number(frac) || 0));
      line = line.slice(m[0].length);
    }
    const clean = line.replace(wordStamp, "").trim();
    if (times.length === 0) plain.push(clean);
    else for (const ms of times) timed.push({ ms, text: clean });
  }
  if (timed.length === 0) {
    // Blank lines at either end are file layout, not verses.
    while (plain.length && plain[0] === "") plain.shift();
    while (plain.length && plain[plain.length - 1] === "") plain.pop();
    return { synced: false, lines: plain.map((t) => ({ ms: -1, text: t })) };
  }
  // A positive offset shows the lines earlier. Sorting is stable, so equal times keep their order.
  const lines = timed.map((l) => ({ ms: Math.max(0, l.ms - offset), text: l.text }));
  lines.sort((a, b) => a.ms - b.ms);
  return { synced: true, lines };
}

/** The line playing at `ms`: the last one started, or -1 before the first. */
export function lineAt(lyrics, ms) {
  if (!lyrics.synced) return -1;
  let i = -1;
  for (let j = 0; j < lyrics.lines.length; j++) {
    if (lyrics.lines[j].ms <= ms) i = j;
    else break;
  }
  return i;
}
