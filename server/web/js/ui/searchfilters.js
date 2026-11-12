// The filters under the Search page's box (docs/plans/041_search_filters_and_popularity.md):
// one chip each, naming what it is set to, opening a dialog to set it.
// Always in view, so what is applied is never hidden.

import { h } from "../util.js";
import { icon } from "../icons.js";
import { dialog, menuAt } from "./kit.js";
import { Query, filterSorts, yearsLabel, names } from "../search.js";
import { ctx } from "./ctx.js";

/** A picker lists this many names at most; typing finds the rest. */
const listed = 200;

/** The chips for filters `f`; `change` takes the filters picked. */
export function filterChips(f, change) {
  // Read when a picker opens: the catalogue may have been replaced since.
  const index = (kind) => ctx.app.catalog.index(kind);
  // A chip shows its value once set, and its name until then.
  const chip = (value, name, onclick) =>
    h(
      "button",
      { type: "button", class: value ? "on" : "", onclick },
      h("span", value || name),
      icon("down"),
    );
  const then = (picked, key) =>
    picked.then((v) => v !== undefined && change(f.with({ [key]: v })));
  return [
    chip(f.years && yearsLabel(f.years), "Year", () =>
      then(pickYears(f.years), "years"),
    ),
    // The indexes are built when first opened, not with the page.
    chip(names(f.artists), "Artist", () =>
      then(pickNames("Artists", index("artists"), f.artists), "artists"),
    ),
    chip(names(f.genres), "Genre", () =>
      then(pickNames("Genres", index("genres"), f.genres), "genres"),
    ),
    h(
      "button",
      {
        type: "button",
        class: f.popular ? "on" : "",
        "aria-pressed": String(f.popular),
        onclick: () => change(f.with({ popular: !f.popular })),
      },
      "Popular",
    ),
    chip(f.sort === "best" ? "" : filterSorts[f.sort], "Sort", (e) =>
      menuAt(
        e.currentTarget,
        Object.entries(filterSorts).map(([sort, label]) => ({
          label,
          action: () => change(f.with({ sort })),
        })),
        "Sort songs",
      ),
    ),
  ];
}

/** A row of a picker's list that picks at once and closes it. */
const pickRow = (label, on, pick) =>
  h(
    "button.row",
    {
      type: "button",
      "aria-current": on ? "true" : null,
      onclick: (e) => {
        pick();
        e.currentTarget.closest("dialog").close();
      },
    },
    h("span.main", label),
    on ? icon("check") : null,
  );

/** Decades a click away, or any range typed in. Resolves with `[from, to]`, null for any year, or undefined. */
async function pickYears(years) {
  const now = new Date().getFullYear();
  let picked;
  const option = (label, r) =>
    pickRow(label, years?.[0] === r[0] && years?.[1] === r[1], () => (picked = r));
  const decades = [];
  for (let d = Math.floor(now / 10) * 10; d >= 1950; d -= 10)
    decades.push(option(yearsLabel([d, d + 9]), [d, d + 9]));
  const field = (label, v) =>
    h("input", {
      type: "number",
      placeholder: label,
      "aria-label": label,
      value: v ?? "",
    });
  const from = field("From", years?.[0]);
  const to = field("To", years?.[1]);
  const r = await dialog(
    "Year",
    [
      h("div.picklist", decades, option("Before 1950", [1, 1949])),
      h("div.yearrange", from, "–", to),
    ],
    [
      { label: "Any year", value: "any" },
      { label: "Apply", submit: true, primary: true, value: "apply" },
    ],
  );
  if (picked) return picked;
  if (r === "any") return null;
  if (r !== "apply") return undefined;
  const a = parseInt(from.value, 10);
  const b = parseInt(to.value, 10);
  if (isNaN(a) && isNaN(b)) return null;
  const [x, y] = [isNaN(a) ? 1 : a, isNaN(b) ? now : b];
  return [Math.min(x, y), Math.max(x, y)];
}

/**
 * Several artists or genres, picked from the library's list and searched as
 * the page searches them. The ones picked stay at the top, then the most
 * songs first: the names people look for are the big ones. Resolves with the
 * names, or undefined.
 */
async function pickNames(title, index, picked) {
  const chosen = new Map(picked.map((n) => [n.toLowerCase(), n]));
  const first = picked
    .map((n) => index.items.find((g) => g.id === n.toLowerCase()))
    .filter(Boolean);
  const list = h("div.picklist");
  const search = h("input", {
    type: "search",
    placeholder: "Search",
    "aria-label": `Search ${title.toLowerCase()}`,
    autocomplete: "off",
    spellcheck: false,
  });
  const draw = () => {
    const rest = index
      .filter(new Query(search.value))
      .filter((g) => !first.includes(g))
      .sort((a, b) => b.songs.length - a.songs.length);
    const shown = [...first, ...rest];
    list.replaceChildren(
      ...shown.slice(0, listed).map((g) =>
        h(
          "label.row",
          h("input", {
            type: "checkbox",
            checked: chosen.has(g.id),
            onchange: (e) =>
              e.target.checked ? chosen.set(g.id, g.name) : chosen.delete(g.id),
          }),
          h("span.main", g.name),
          h("span.count", String(g.songs.length)),
        ),
      ),
      shown.length > listed
        ? h("p.picknote", `${shown.length - listed} more: type to find them`)
        : null,
    );
  };
  search.addEventListener("input", draw);
  draw();
  const r = await dialog(
    title,
    [search, list],
    [
      { label: "Clear", value: "clear" },
      { label: "Apply", submit: true, primary: true, value: "apply" },
    ],
    { cls: "wide" },
  );
  if (r === "clear") return [];
  return r === "apply" ? [...chosen.values()] : undefined;
}
