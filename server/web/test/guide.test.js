// How it works (plan 034): the renderer, and the ⓘ links into it.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { guideHtml, slug } from "../js/guide.js";

const read = (p) => readFileSync(new URL(p, import.meta.url), "utf8");

test("the guide's text is escaped before its bold is marked", () => {
  assert.equal(
    guideHtml("A <b>tag</b> & **bold**"),
    "<p>A &lt;b&gt;tag&lt;/b&gt; &amp; <strong>bold</strong></p>",
  );
});

test("a list item's wrapped line stays in the item", () => {
  assert.equal(
    guideHtml("- **Bar:** a strip\n  along the bottom.\n- None"),
    "<ul><li><strong>Bar:</strong> a strip along the bottom.</li><li>None</li></ul>",
  );
});

test("every ⓘ in Settings names a heading the guide has", () => {
  const html = guideHtml(read("../guide.md"));
  const links = [...read("../js/ui/settings.js").matchAll(/guide: "([^"]+)"/g)];
  assert.ok(links.length > 0);
  for (const [, name] of links) assert.ok(html.includes(`id="${slug(name)}"`), name);
});
