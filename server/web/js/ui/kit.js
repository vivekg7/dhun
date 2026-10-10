// The pieces every page uses: a virtual list, menus, dialogs, toasts.

import { h } from "../util.js";
import { icon } from "../icons.js";

/** Phones and tablets: a tap plays, there is no hover, and menus open as sheets. */
export const touch = () => matchMedia("(pointer: coarse)").matches;
export const reducedMotion = () =>
  matchMedia("(prefers-reduced-motion: reduce)").matches;

/** An icon-only button; its name shows on hover and is what a screen reader says. */
export function iconButton(name, label, onclick, cls = "") {
  return h(
    "button.icon-button",
    { class: cls, title: label, "aria-label": label, type: "button", onclick },
    icon(name),
  );
}

// MARK: The virtual list

/**
 * Only the rows on screen exist, so 7,000 songs scroll like twenty. `el` is
 * the scroller; `head` (a page's header) scrolls with the rows above them.
 * Rows are one fixed height, read from the CSS variable --row.
 *
 * Reorder: `reorder(from, to)` makes each row's `.handle` draggable. The row
 * follows the pointer, the rows it passes slide aside, the list scrolls at
 * its edges, and the new order stays on screen until the items change, so
 * the drop never snaps back (plan 027).
 */
export class VList {
  constructor({ render, head, foot, cls = "", reorder, empty }) {
    this.render = render;
    this.reorderTo = reorder;
    this.emptyEl = empty;
    this.items = [];
    this.rows = h("div.vrows");
    this.head = h("div.vhead", head);
    this.foot = h("div.vfoot", foot);
    this.el = h("div.vlist", { class: cls }, this.head, this.rows, this.foot);
    this.el.addEventListener("scroll", () => this.paint(), { passive: true });
    this.shown = new Map();
    new ResizeObserver(() => this.paint(true)).observe(this.el);
  }

  get rowHeight() {
    // From the rows, not the scroller: a container query on the scroller's width sets it there.
    return parseFloat(getComputedStyle(this.rows).getPropertyValue("--row")) || 40;
  }

  setItems(items) {
    this.items = items;
    if (this.drag && !this.drag.active) this.drag = null;
    this.paint(true);
  }

  /** Draws the rows in view; `all` draws them again even if the range is the same. */
  paint(all = false) {
    const rh = this.rowHeight;
    const n = this.items.length;
    this.rows.style.height = `${n * rh}px`;
    if (this.emptyEl) this.emptyEl.hidden = n > 0;
    const top = this.el.scrollTop - this.rows.offsetTop;
    const first = Math.max(0, Math.floor(top / rh) - 8);
    const last = Math.min(n, Math.ceil((top + this.el.clientHeight) / rh) + 8);
    if (!all && first === this.first && last === this.last) return;
    this.first = first;
    this.last = last;
    const keep = new Map();
    for (let i = first; i < last; i++) {
      let row = all ? null : this.shown.get(i);
      if (!row || row.item !== this.items[i]) {
        row = { item: this.items[i], el: this.render(this.items[i], i) };
        row.el.classList.add("vrow");
        row.el.dataset.index = i;
      }
      keep.set(i, row);
      this.place(row.el, i);
    }
    const els = [...keep.values()].map((r) => r.el);
    if (
      all ||
      els.some((e) => e.parentNode !== this.rows) ||
      this.rows.children.length !== els.length
    )
      this.rows.replaceChildren(...els);
    this.shown = keep;
  }

  /** Where row `i` sits: its own place, or out of the way of a row being dragged. */
  place(el, i) {
    const rh = this.rowHeight;
    const d = this.drag;
    let y = i * rh;
    if (d?.active) {
      if (i === d.from) {
        y = d.y;
        el.classList.add("lifted");
      } else {
        if (d.from < d.to && i > d.from && i <= d.to) y -= rh;
        if (d.to < d.from && i >= d.to && i < d.from) y += rh;
      }
    } else el.classList.remove("lifted");
    el.style.transform = `translateY(${y}px)`;
  }

  /** The row at index `i` brought into view. */
  scrollTo(i, center = true) {
    const rh = this.rowHeight;
    const y = this.rows.offsetTop + i * rh;
    if (y >= this.el.scrollTop && y + rh <= this.el.scrollTop + this.el.clientHeight)
      return;
    this.el.scrollTop = center ? y - this.el.clientHeight / 2 + rh / 2 : y;
  }

  /** Call from a handle's pointerdown. */
  startDrag(e, from) {
    if (!this.reorderTo || e.button > 0) return;
    e.preventDefault();
    const rh = this.rowHeight;
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    const offset = e.clientY - (this.rows.getBoundingClientRect().top + from * rh);
    this.drag = { from, to: from, y: from * rh, active: true };
    this.el.classList.add("dragging");
    let lastY = e.clientY;
    const move = (y) => {
      lastY = y;
      const top = this.rows.getBoundingClientRect().top;
      const d = this.drag;
      d.y = Math.max(0, Math.min((this.items.length - 1) * rh, y - top - offset));
      d.to = Math.max(0, Math.min(this.items.length - 1, Math.round(d.y / rh)));
      for (const [i, row] of this.shown) this.place(row.el, i);
    };
    // Held near an edge, the list scrolls, faster nearer the edge.
    const edge = () => {
      if (!this.drag?.active) return;
      const r = this.el.getBoundingClientRect();
      const zone = 56;
      const speed =
        lastY < r.top + zone
          ? -(r.top + zone - lastY)
          : lastY > r.bottom - zone
            ? lastY - (r.bottom - zone)
            : 0;
      if (speed) {
        this.el.scrollTop += speed / 3;
        this.paint();
        move(lastY);
      }
      requestAnimationFrame(edge);
    };
    requestAnimationFrame(edge);
    const onMove = (ev) => move(ev.clientY);
    const onUp = () => {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
      const d = this.drag;
      d.active = false;
      this.el.classList.remove("dragging");
      if (d.to !== d.from) {
        // Shown in the new order at once; the items that come back replace it.
        const items = this.items.slice();
        items.splice(d.to, 0, ...items.splice(d.from, 1));
        this.items = items;
        this.drag = null;
        this.paint(true);
        this.reorderTo(d.from, d.to);
      } else {
        this.drag = null;
        this.paint(true);
      }
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
  }
}

// MARK: Menus

let openMenu = null;

export function closeMenu() {
  openMenu?.close();
}

/**
 * A menu at the pointer, or a sheet from the bottom on a touch screen.
 * Items: {label, action, disabled, danger, items (a submenu)}, or "-".
 */
export function showMenu(items, at, title) {
  closeMenu();
  const sheet = touch() || innerWidth < 600;
  const backdrop = h("div.menu-backdrop", { class: sheet ? "sheet" : "" });
  const build = (list, heading) => {
    const m = h("div.menu", { role: "menu", tabindex: "-1" });
    if (heading) m.append(h("div.menu-title", heading));
    for (const it of list) {
      if (it === "-") {
        m.append(h("div.menu-sep"));
        continue;
      }
      if (!it) continue;
      const b = h(
        "button.menu-item",
        {
          role: "menuitem",
          type: "button",
          disabled: it.disabled,
          class: it.danger ? "danger" : "",
        },
        h("span", it.label),
        it.items ? icon("right") : null,
      );
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        if (it.items) {
          const sub = build(it.items, sheet ? it.label : null);
          if (sheet) {
            m.replaceWith(sub);
            sub.focus();
          } else {
            m.querySelector(".menu.sub")?.remove();
            sub.classList.add("sub");
            backdrop.append(sub);
            const r = b.getBoundingClientRect();
            position(sub, r.right - 4, r.top - 6, r.left + 4);
            sub.querySelector(".menu-item")?.focus();
          }
          return;
        }
        close();
        it.action?.();
      });
      b.addEventListener("mouseenter", () => {
        b.focus();
        if (sheet || m.classList.contains("sub")) return;
        // In the top menu, a submenu opens under the pointer and any other closes.
        if (it.items && !it.disabled) b.click();
        else backdrop.querySelectorAll(".menu.sub").forEach((sub) => sub.remove());
      });
      m.append(b);
    }
    m.addEventListener("keydown", (e) => {
      const buttons = [...m.querySelectorAll(":scope > .menu-item:not([disabled])")];
      const i = buttons.indexOf(document.activeElement);
      if (e.key === "ArrowDown") buttons[(i + 1) % buttons.length]?.focus();
      else if (e.key === "ArrowUp")
        buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
      else if (
        e.key === "ArrowRight" &&
        document.activeElement?.lastChild?.tagName === "svg"
      )
        document.activeElement.click();
      else if (e.key === "ArrowLeft" && m.classList.contains("sub")) {
        m.remove();
      } else return;
      e.preventDefault();
    });
    return m;
  };
  const m = build(items, sheet ? title : null);
  backdrop.append(m);
  const close = () => {
    backdrop.remove();
    removeEventListener("keydown", onKey, true);
    removeEventListener("resize", close);
    openMenu = null;
    restore?.focus?.();
  };
  const onKey = (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  };
  const restore = document.activeElement;
  backdrop.addEventListener("pointerdown", (e) => e.target === backdrop && close());
  backdrop.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (e.target === backdrop) close();
  });
  addEventListener("keydown", onKey, true);
  addEventListener("resize", close);
  document.body.append(backdrop);
  if (!sheet) position(m, at.x, at.y);
  m.querySelector(".menu-item:not([disabled])")?.focus({ preventScroll: true });
  openMenu = { close };
}

/** Puts a menu at (x, y), flipped to stay on screen; `flipX` is where it goes when there is no room on the right. */
function position(m, x, y, flipX = x) {
  const r = m.getBoundingClientRect();
  const left = x + r.width > innerWidth - 8 ? Math.max(8, flipX - r.width) : x;
  const top =
    y + r.height > innerHeight - 8 ? Math.max(8, innerHeight - 8 - r.height) : y;
  m.style.left = `${left}px`;
  m.style.top = `${top}px`;
}

/** A menu under a button. */
export function menuAt(button, items, title) {
  const r = button.getBoundingClientRect();
  showMenu(items, { x: r.left, y: r.bottom + 4 }, title);
}

/**
 * A panel by a button (the speed and sleep controls), or a sheet on a phone.
 * `build(close)` makes its content; the panel closes on Esc or a click outside.
 */
export function popover(button, build) {
  closeMenu();
  const sheet = innerWidth < 600;
  const backdrop = h("div.menu-backdrop", { class: sheet ? "sheet" : "" });
  const panel = h("div.popover", { role: "dialog" });
  const close = () => {
    backdrop.remove();
    removeEventListener("keydown", onKey, true);
    openMenu = null;
    button.focus?.({ preventScroll: true });
  };
  const onKey = (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  };
  panel.append(build(close));
  backdrop.append(panel);
  backdrop.addEventListener("pointerdown", (e) => e.target === backdrop && close());
  addEventListener("keydown", onKey, true);
  document.body.append(backdrop);
  if (!sheet) {
    const r = button.getBoundingClientRect();
    const p = panel.getBoundingClientRect();
    const above = r.top - p.height - 8 > 8;
    panel.style.left = `${Math.max(8, Math.min(innerWidth - p.width - 8, r.left + r.width / 2 - p.width / 2))}px`;
    panel.style.top = `${above ? r.top - p.height - 8 : r.bottom + 8}px`;
  }
  panel.querySelector("button, input")?.focus({ preventScroll: true });
  openMenu = { close };
}

// MARK: Dialogs

/** A modal with `body` and buttons; resolves with the value of the button pressed, or null. */
export function dialog(title, body, buttons, { onOpen, cls = "" } = {}) {
  return new Promise((resolve) => {
    const form = h("form.dialog-body", { method: "dialog" });
    const d = h("dialog.dialog", { class: cls }, h("h2", title), form);
    form.append(...(Array.isArray(body) ? body : [body]));
    const row = h("div.dialog-buttons");
    for (const b of buttons) {
      row.append(
        h(
          "button",
          {
            type: b.submit ? "submit" : "button",
            class: b.primary ? "primary" : b.danger ? "danger" : "",
            value: b.value ?? "",
            onclick: b.submit ? null : () => finish(b.value ?? null),
          },
          b.label,
        ),
      );
    }
    form.append(row);
    let result = null;
    const finish = (v) => {
      result = v;
      d.close();
    };
    let busy = false;
    form.addEventListener("submit", async (e) => {
      e.preventDefault();
      const b = buttons.find((x) => x.submit);
      if (busy) return;
      // A check may ask the server (adding a member): the dialog stays until it answers.
      busy = true;
      const ok = b?.check ? await b.check() : true;
      busy = false;
      if (ok) finish(b?.value ?? true);
    });
    d.addEventListener("close", () => {
      d.remove();
      resolve(result);
    });
    document.body.append(d);
    d.showModal();
    onOpen?.(d);
  });
}

/** Asks for a name. `check` returns an error to show, or "" when the name will do. */
export async function prompt(
  title,
  initial = "",
  { action = "OK", check = () => "", label = "Name" } = {},
) {
  const input = h("input", {
    type: "text",
    value: initial,
    "aria-label": label,
    autocomplete: "off",
    spellcheck: false,
  });
  const error = h("p.dialog-error");
  const ok = () => {
    const v = input.value.trim();
    error.textContent = v ? check(v) : "";
    return !!v && !error.textContent;
  };
  const r = await dialog(
    title,
    [input, error],
    [
      { label: "Cancel" },
      { label: action, submit: true, primary: true, value: "ok", check: ok },
    ],
    {
      onOpen: () => {
        input.focus();
        input.select();
      },
    },
  );
  return r === "ok" ? input.value.trim() : null;
}

export async function confirm(title, detail, action, danger = true) {
  const r = await dialog(title, detail ? h("p", detail) : [], [
    { label: "Cancel" },
    { label: action, submit: true, danger, primary: !danger, value: "yes" },
  ]);
  return r === "yes";
}

// MARK: Toasts

/** A short note at the bottom, gone after a few seconds. */
export function toast(text) {
  let host = document.querySelector(".toasts");
  if (!host)
    document.body.append(
      (host = h("div.toasts", { role: "status", "aria-live": "polite" })),
    );
  const t = h(
    "div.toast",
    h("span", text),
    iconButton("close", "Dismiss", () => gone()),
  );
  const gone = () => {
    t.classList.add("leaving");
    setTimeout(() => t.remove(), 200);
  };
  host.append(t);
  setTimeout(gone, 4000);
}

/** Shown while there is nothing to show. */
export const emptyNote = (title, detail, name = "note") =>
  h(
    "div.empty",
    icon(name, "big"),
    h("p.empty-title", title),
    detail ? h("p.empty-detail", detail) : null,
  );
