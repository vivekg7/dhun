// Sign-in, and Settings: the apps' pages in one (plans 018, 023, 030).

import { h, ago } from "../util.js";
import { icon } from "../icons.js";
import { confirm, dialog } from "./kit.js";
import { frame } from "./pages.js";
import { formatSpeed } from "../queue.js";
import { ctx } from "./ctx.js";

/** The phone's eight accent palettes (plan 011), Dull Orange first, with a lighter shade for dark mode. */
export const palettes = [
  ["Dull Orange", "#C27B45", "#D9925C"],
  ["Sage", "#7E9C7A", "#98B594"],
  ["Slate Blue", "#6A7FA8", "#8C9FC6"],
  ["Teal", "#4E9A96", "#6FB6B1"],
  ["Dusty Rose", "#B9707F", "#D08D9B"],
  ["Mauve", "#9479A8", "#B097C2"],
  ["Olive", "#8F9152", "#ADAF6F"],
  ["Sand", "#B59E73", "#CBB68C"],
];

/** The theme and accent on the page: this browser's own, as each device has its own. */
export function applyTheme(prefs) {
  const root = document.documentElement;
  const [, light, dark] =
    palettes[Math.min(Math.max(prefs.palette ?? 0, 0), palettes.length - 1)];
  root.style.setProperty("--accent-light", light);
  root.style.setProperty("--accent-dark", dark);
  root.dataset.theme = prefs.theme ?? "system";
  const meta = document.querySelector('meta[name="theme-color"]');
  const isDark =
    prefs.theme === "dark" ||
    (prefs.theme !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  meta?.setAttribute("content", isDark ? "#1c1b1a" : "#fbfaf8");
}

export function signInPage(onDone) {
  const app = ctx.app;
  const user = h("input", {
    type: "text",
    name: "username",
    autocomplete: "username",
    value: app.prefs.user ?? "",
    required: true,
    autocapitalize: "off",
    spellcheck: false,
  });
  const password = h("input", {
    type: "password",
    name: "password",
    autocomplete: "current-password",
    required: true,
  });
  const error = h("p.error", { role: "alert" });
  const button = h("button.primary", { type: "submit" }, "Sign in");
  const form = h(
    "form.signin",
    h("img.logo", { src: "icon.svg", alt: "" }),
    h("h1", "Sign in to Dhun"),
    app.revoked
      ? h(
          "p.note",
          "This browser was signed out on the server. Sign in again; nothing you changed is lost.",
        )
      : null,
    h("label", h("span", "Name"), user),
    h("label", h("span", "Password"), password),
    error,
    button,
  );
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    button.disabled = true;
    error.textContent = "";
    try {
      await app.signIn(user.value.trim(), password.value);
      onDone();
    } catch (err) {
      error.textContent =
        err.status === 401
          ? "Wrong name or password."
          : err.status === 429
            ? "Too many tries. Wait a little, then try again."
            : err.message;
      button.disabled = false;
    }
  });
  queueMicrotask(() => (user.value ? password : user).focus());
  return h("div.signin-page", form);
}

/** A row of a settings group: a label, maybe a line under it, and its control. */
const row = (label, control, note) =>
  h(
    "div.setting",
    h("div.settinglabel", h("span", label), note ? h("small", note) : null),
    control,
  );

function select(options, value, onChange) {
  const s = h(
    "select",
    { onchange: () => onChange(s.value) },
    options.map(([v, label]) =>
      h("option", { value: String(v), selected: String(v) === String(value) }, label),
    ),
  );
  return s;
}

function toggle(on, onChange) {
  const t = h("input.switch", { type: "checkbox", checked: on, role: "switch" });
  t.addEventListener("change", () => onChange(t.checked));
  return t;
}

export function settingsPage() {
  const body = h("div.scroll.settings");
  const draw = () => {
    const app = ctx.app;
    const p = ctx.playback;
    const synced = (name, def, parse = (v) => v) => [
      app.setting(name, def),
      (v) => app.setSetting(name, parse(v)),
    ];
    const [minutes, setMinutes] = synced("longFiles.minMinutes", 15, Number);
    const [resume, setResume] = synced("longFiles.resume", "auto");
    const [autoRemove, setAutoRemove] = synced("listenLater.autoRemove", true);
    const [percent, setPercent] = synced("listenLater.finishedPercent", 90, Number);
    const palette = app.prefs.palette ?? 0;
    body.replaceChildren(
      h(
        "section.group",
        h("h2", "Appearance"),
        row(
          "Theme",
          select(
            [
              ["system", "Follow the system"],
              ["light", "Light"],
              ["dark", "Dark"],
            ],
            app.prefs.theme ?? "system",
            (v) => {
              app.setPref("theme", v === "system" ? undefined : v);
              applyTheme(app.prefs);
            },
          ),
        ),
        row(
          "Accent colour",
          h(
            "div.swatches",
            palettes.map(([name, light, dark], i) =>
              h("button.swatch", {
                type: "button",
                title: name,
                "aria-label": name,
                "aria-pressed": String(i === palette),
                style: { "--light": light, "--dark": dark },
                onclick: () => {
                  app.setPref("palette", i || undefined);
                  applyTheme(app.prefs);
                  draw();
                },
              }),
            ),
          ),
        ),
      ),
      h(
        "section.group",
        h("h2", "Long files"),
        h(
          "p.groupnote",
          "Audiobooks, podcasts and mixes this long continue where you left them, on all your devices.",
        ),
        row(
          "Continue long files",
          select(
            [5, 10, 15, 20, 30, 60].map((m) => [m, `${m} minutes and longer`]),
            minutes,
            setMinutes,
          ),
        ),
        row(
          "Playing one again",
          select(
            [
              ["auto", "Continue where you left it"],
              ["ask", "Ask"],
              ["off", "Start over"],
            ],
            resume,
            setResume,
          ),
        ),
      ),
      h(
        "section.group",
        h("h2", "Listen Later"),
        row(
          "Remove what you finish",
          toggle(autoRemove, (v) => (setAutoRemove(v), draw())),
        ),
        row(
          "Finished at",
          Object.assign(
            select(
              [80, 90, 95, 100].map((n) => [n, `${n}% heard`]),
              percent,
              setPercent,
            ),
            { disabled: !autoRemove },
          ),
        ),
      ),
      h(
        "section.group",
        h("h2", "Speed"),
        row(
          "Everyday in this browser",
          h(
            "span.inline",
            h(
              "span",
              app.prefs.speed && app.prefs.speed !== 1
                ? `${formatSpeed(app.prefs.speed)}×`
                : "Normal",
            ),
            h(
              "button",
              {
                type: "button",
                disabled: !app.prefs.speed || app.prefs.speed === 1,
                onclick: () => (p.setSpeed(1, false), draw()),
              },
              "Reset",
            ),
          ),
          "Set from the speed button by Now playing. The pitch can be changed in the phone and Mac apps.",
        ),
      ),
      h(
        "section.group",
        h("h2", "Account"),
        row("Signed in as", h("span", `${app.user}${app.admin ? " (admin)" : ""}`)),
        app.prefs.serverVersion
          ? row(
              "Dhun version",
              h("span", app.prefs.serverVersion),
              "The server’s; the web client comes with it.",
            )
          : null,
        row(
          "Last sync",
          h(
            "span.inline",
            h(
              "span",
              { class: app.syncError ? "error" : "" },
              app.syncError ??
                (app.syncing ? "Syncing…" : app.reachable ? "Up to date" : "Offline"),
            ),
            h("button", { type: "button", onclick: () => app.syncNow() }, "Sync now"),
          ),
        ),
        row(
          "Your devices",
          h("button", { type: "button", onclick: devicesDialog }, "Show…"),
          "Sign out a lost phone or an old browser.",
        ),
        app.admin
          ? row(
              "Family members",
              h("button", { type: "button", onclick: membersDialog }, "Manage…"),
            )
          : null,
        row(
          "Sign out",
          h(
            "button.danger",
            {
              type: "button",
              onclick: async () => {
                if (
                  await confirm(
                    "Sign out?",
                    "This browser forgets your queues and any changes not yet sent.",
                    "Sign out",
                  )
                )
                  app.signOut();
              },
            },
            "Sign out…",
          ),
        ),
      ),
      h(
        "section.group",
        h("h2", "Keys"),
        h(
          "dl.keys",
          [
            ["Space", "Play or pause"],
            ["⌘ ← / ⌘ →  (Ctrl on Windows and Linux)", "Previous and next song"],
            ["/  or  ⌘ K", "Search"],
            ["Return", "Play the selected song"],
            ["Delete", "Remove the selected songs from a queue or playlist"],
          ].map(([k, v]) => [h("dt", k), h("dd", v)]),
        ),
      ),
      h(
        "section.group",
        h("h2", "About"),
        h(
          "p.groupnote",
          "Dhun, a music system for one family. ",
          h(
            "a",
            {
              href: "https://github.com/vivekg7/dhun",
              target: "_blank",
              rel: "noopener",
            },
            "Source code (GPL-3.0)",
          ),
        ),
      ),
    );
  };
  draw();
  return {
    el: frame({ title: "Settings", body }),
    title: "Settings",
    topics: new Set(["settings", "sync", "account"]),
    update: draw,
    scroller: body,
  };
}

/** The user's signed-in devices; one can be signed out (a lost phone). */
async function devicesDialog() {
  const app = ctx.app;
  const list = h("div.memberlist", h("div.spinner"));
  const error = h("p.dialog-error");
  const load = async () => {
    try {
      const devices = await app.devices();
      list.replaceChildren(
        ...devices.map((d) =>
          h(
            "div.member",
            h(
              "div",
              h("b", d.name || "Unnamed"),
              h(
                "small",
                d.current ? "This browser" : `Seen ${ago(Date.parse(d.lastSeenAt))}`,
              ),
            ),
            d.current
              ? null
              : h(
                  "button",
                  {
                    type: "button",
                    onclick: async () => {
                      if (
                        !(await confirm(
                          `Sign out “${d.name}”?`,
                          "It will need your password to sign in again. Changes it has not sent are lost.",
                          "Sign out",
                        ))
                      )
                        return;
                      await app
                        .removeDevice(d.id)
                        .catch((e) => (error.textContent = e.message));
                      load();
                    },
                  },
                  "Sign out…",
                ),
          ),
        ),
      );
    } catch (e) {
      error.textContent = e.message;
    }
  };
  load();
  await dialog(
    "Your devices",
    [list, error],
    [{ label: "Done", submit: true, primary: true }],
    { cls: "wide" },
  );
}

/** Family members, for the admin (plan 023): list, add, reset a forgotten password. */
async function membersDialog() {
  const app = ctx.app;
  const list = h("div.memberlist", h("div.spinner"));
  const error = h("p.dialog-error");
  const seen = (m) => {
    if (m.admin) return "Its password is changed on the NAS";
    if (!m.devices) return "No device signed in";
    const on = `Signed in on ${m.devices} device${m.devices === 1 ? "" : "s"}`;
    return m.lastSeenAt ? `${on} · seen ${ago(Date.parse(m.lastSeenAt))}` : on;
  };
  const load = async () => {
    try {
      const members = await app.members();
      list.replaceChildren(
        ...members.map((m) =>
          h(
            "div.member",
            h("div", h("b", m.name + (m.admin ? " (admin)" : "")), h("small", seen(m))),
            m.admin
              ? null
              : h(
                  "button",
                  {
                    type: "button",
                    onclick: () =>
                      memberForm(`New password for ${m.name}`, false, (_, pw) =>
                        app.resetPassword(m.id, pw),
                      ).then(load),
                  },
                  "Reset password…",
                ),
          ),
        ),
      );
      error.textContent = "";
    } catch (e) {
      error.textContent = e.message;
    }
  };
  load();
  const add = h(
    "button",
    {
      type: "button",
      onclick: () =>
        memberForm("Add a family member", true, (n, pw) => app.addMember(n, pw)).then(
          load,
        ),
    },
    icon("add"),
    "Add member…",
  );
  await dialog(
    "Family members",
    [list, error, add],
    [{ label: "Done", submit: true, primary: true }],
    { cls: "wide" },
  );
}

/**
 * A name (when adding) and a password. The server's rules for both come back
 * as its own error, rather than copied here where they could drift.
 */
function memberForm(title, askName, save) {
  const name = h("input", {
    type: "text",
    placeholder: "Name",
    "aria-label": "Name",
    autocomplete: "off",
  });
  // Shown as typed: the admin reads it out to the member, and a typo in a hidden password would lock them out.
  const password = h("input", {
    type: "text",
    placeholder: "Password",
    "aria-label": "Password",
    autocomplete: "off",
    spellcheck: false,
  });
  const error = h("p.dialog-error");
  const check = async () => {
    try {
      await save(name.value.trim(), password.value);
      return true;
    } catch (e) {
      error.textContent = e.message;
      return false;
    }
  };
  return dialog(
    title,
    [askName ? name : null, password, error].filter(Boolean),
    [
      { label: "Cancel" },
      { label: "Save", submit: true, primary: true, value: "save", check },
    ],
    {
      onOpen: () => (askName ? name : password).focus(),
    },
  );
}
