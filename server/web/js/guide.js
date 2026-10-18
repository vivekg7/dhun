// How it works (docs/plans/034_settings_text_defaults_reset.md): the little
// Markdown docs/GUIDE.md is written in, as HTML. Headings, paragraphs, "- "
// lists and **bold**, which is all the guide uses; no library.

/** A heading's id, from its text: "Previous button" → "guide-previous-button". */
export const slug = (text) =>
  "guide-" +
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

const escape = (s) =>
  s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

const inline = (s) => escape(s).replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");

export function guideHtml(md) {
  const out = [];
  let para = [];
  let list = null;
  const flush = () => {
    if (para.length) out.push(`<p>${inline(para.join(" "))}</p>`);
    para = [];
    if (list) out.push(`<ul>${list.map((i) => `<li>${inline(i)}</li>`).join("")}</ul>`);
    list = null;
  };
  for (const line of md.split("\n")) {
    const heading = /^(#{1,3}) (.+)$/.exec(line);
    if (heading) {
      flush();
      const [, hashes, text] = heading;
      out.push(
        `<h${hashes.length} id="${slug(text)}">${inline(text)}</h${hashes.length}>`,
      );
    } else if (line.startsWith("- ")) {
      if (para.length) flush();
      (list ??= []).push(line.slice(2));
    } else if (!line.trim()) {
      flush();
    } else if (list && /^\s/.test(line)) {
      list[list.length - 1] += ` ${line.trim()}`;
    } else {
      if (list) flush();
      para.push(line.trim());
    }
  }
  flush();
  return out.join("\n");
}
