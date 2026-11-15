# Web UI design guide

The design guide of the web UI served by `pai-bff` (`fe/`). Kept by the `designer` agent; the coder
and the tester apply it, and every slice brief (`fe/briefs/`) builds on it. Rules marked
**(preference)** are design choices, not facts; everything else names its source.

The reference for what each task does is the local configurator `pai-config` (`crates/config/`, its
texts in `crates/core/strings/en_US.toml`). The web UI does the same tasks with the same names,
order, defaults and checks, and says where it differs on purpose.

## 1. Principles

1. **Same task, same words as `pai-config`.** A field the GUI calls "Max models" is "Max models"
   here, in the same card, in the same order, with the same default.
2. **The server decides.** The fe checks a value only to answer fast (a whole number in the range the
   API gave, no `;` or `#` in a path). Every rule is enforced again by the bff and the manager; a
   refusal comes back as a strings key plus args, and the fe shows it in its own text.
3. **Nothing about the browser's machine.** CPU counts, network interfaces, default folders and
   devices are the server machine's, read from the API. The fe never resolves a path or a default
   itself.
4. **No empty chrome.** A tab list with one tab, a filter with one choice, a nav entry for a page
   that does not exist yet: none of these are shown. The structure has a place for them (section 2);
   they appear when their second item or their page does. The exception is a value the user must
   declare, such as a preset's engine (section 2.2): its select shows even with one entry.
5. **Smallest design that does the job.** Reuse a component of section 5 before adding one; add a
   token or a pattern here only when a slice needs it.

## 2. Information architecture

### 2.1 The map

Every page of the finished web UI, in nav order. "Slice" says when it is built; a page with no slice
is not in the nav yet.

| Nav entry | Route | What it holds | `pai-config` equivalent | Who | Slice |
|---|---|---|---|---|---|
| Overview | `#/` | Framework version; the instances with engine and state, each a link | (none; the footer status) | anyone | 1a (moves today's page) |
| Instances > *instance* | `#/instances/<id>` | Header: name, engine, state badge, Start / Stop / Restart, start at boot. Below, the instance's pages as tabs once there are two: **Server** (server file), **Logs** | Server tab, run controls, Settings' "start at boot", View logs | signed in | 1a (read), 1b (save), 1c (run controls); Logs later |
| Models | `#/models`, `#/models/<id>` | The one mixed preset list (every engine), each row with its engine and on/off state; the preset editor, its first field the engine (section 2.2) | Models tab | signed in; edit: administrator | later |
| Downloads | `#/downloads` | Hugging Face search, candidates, running downloads | HF dialog, Downloads window | administrator (writes the models folder) | later, **open** (below) |
| Benchmarks | `#/benchmarks` | Run a benchmark, saved runs, reports | Benchmark tab | signed in | later, **open** (below) |
| Results | `#/results` | The benchmark portal: published campaigns from `bench.db`, read-only | (none) | anyone (to decide) | later |
| Hardware | `#/hardware` | Devices, live GPU / CPU / memory / disk monitor of the server machine | HW Monitor | signed in | later |
| Integrations | `#/integrations` | OpenCode and Claude Code snippets built from the server settings | Integrations tab | signed in | later, **open** (below) |
| Gateway | `#/gateway` | The OpenAI-compatible endpoint in front of the instances: keys, routing | (none) | administrator | later |
| Settings | `#/settings` | The web user's own preferences | Settings tab | signed in | later, **open** (below) |

Deliberate moves from the GUI:

- **Start at boot is on the instance header, not in Settings.** It is the instance service's state
  (manager `BootEnabled` / `SetBootEnabled`), not a preference of the user, and each instance has its
  own.
- **Settings keeps only per-user things.** Start at login, start minimized, window sizes and the
  "start llama-server when pai-config launches" switch have no meaning on the web and are dropped.
  Log rotation applies only where the configurator spawns llama-server itself (macOS), and the bff
  does not run on macOS: dropped.
- **Restart exists** (the GUI has none; `pai control restart` does).

**Open: per-user state with no home yet.** `settings.ini`, `bench.db` (and the benchmark's prompt
and database choice), the Hugging Face token, and `opencode.json` are per OS user and live in the
user's home, which the bff's account cannot reach (`ProtectHome`; inventory section 4.1). Until the
user decides where a web user's state lives, Downloads, Benchmarks, Integrations and Settings stay
out of the nav. Integrations can ship earlier as read-only snippets (copy, no write).

### 2.2 Engines and instances

Decided with the user on 2026-11-14.

- **Instances** belong to an engine (the service `llama.cpp@<id>`). The route uses the bff's instance
  id (`#/instances/default`, as `/api/instances/{id}/...`). The page shows the id as the heading and
  "`<engine>` instance" under it: the engine is a fact about the instance, not a control.
- The Overview's table gets an Engine column only when the instances span two or more engines.
- **Presets** are one mixed list for every engine. Each preset declares its engine, chosen from the
  **available engines** the bff lists (today only llama.cpp):
  - The preset editor's first field is **Engine**, a `<select>` over that list, never free text, shown
    even with one entry (user decision: it is how a preset declares its engine).
  - A preset that names no engine is a llama.cpp preset: the editor shows llama.cpp selected, no
    migration prompt (every preset written before engines existed).
  - **Engine not available**: a preset naming an engine the list lacks (e.g. Strata uninstalled). The
    list row shows the engine name with the text "not available"; the editor shows the select with that
    name as an extra, selected option marked "(not available)" and a Notice that the preset cannot be
    started until the engine is installed or another engine is chosen. Its other fields stay readable;
    fields of an engine the fe does not know are shown read-only as raw `key = value` lines, kept on
    save.
  - The preset list's Engine column is always shown (each row has an engine), sorted with the list.
- Engine names are data (`llama.cpp`, `Strata`): never in the strings file, never translated.

### 2.3 Navigation

- Hash routes (`#/...`): the bff serves only `/` and `/assets/*`, so the fe routes on the fragment
  and the bff needs no fallback route.
- The nav lists only pages that exist. Under "Instances" one link per instance, from
  `GET /api/instances`, in the manager's order.
- The current page's nav link has `aria-current="page"`.
- On every route change: `document.title` becomes `<page> | Private AI Framework`
  ([2.4.2 Page Titled](https://www.w3.org/TR/WCAG22/#page-titled)), and focus moves to the page's
  `h1` (`tabindex="-1"`), so a screen reader announces the new page
  ([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)).
- Leaving a page with unsaved changes asks first (section 6, "Unsaved changes"); closing the tab
  triggers the browser's own `beforeunload` prompt.

## 3. Who is acting: sign-in and roles

The user signs in with their OS account; the bff forwards the identity to `private-ai-manager`,
which applies its rules (user decision, 2026-11-14; `AGENTS.md` and `PLAN.md` step 10).

| Role | How the fe knows | Sees | Can do |
|---|---|---|---|
| Signed out | `GET /api/session` answers 401 | Overview only; header shows **Sign in** | read version and instance states |
| User | session with `canConfigure: false` | everything signed-in pages show, settings read-only with a notice | start, stop, restart (manager `control`) |
| Administrator | session with `canConfigure: true` | the same, editable, with the save bar | also save files and set start at boot (`configure`) |

- `canConfigure` only shapes the page. The manager decides each request; a refusal is shown as
  `not-authorized` wherever it lands, even for an administrator.
- A signed-out user who opens any page but the Overview sees the sign-in form in its place, and
  returns to that page after signing in.
- The session can end while a page is open (any request answers 401). The page keeps what
  it shows and any unsaved edits, the header and nav stay as they were, and the page offers
  **Sign in** in a dialog; after it, the user retries. Signing in there as a different user, or as one
  without `canConfigure`, drops the edits (they belong to the session that made them): the page reads
  its data again for the new session and says, in an alert, that the changes were dropped.
- Sign out is in the user menu of the header; with unsaved changes it asks first.
- The sign-in form lets password managers and paste work: `autocomplete="username"` and
  `"current-password"`, no paste blocking, no puzzle
  ([3.3.8 Accessible Authentication (Minimum)](https://www.w3.org/TR/WCAG22/#accessible-authentication-minimum)).

## 4. Layout and responsive rules

- **App shell**: a header (product name, user menu or Sign in), a nav, and the main region.
  Landmarks: `header` (banner), `nav` with an accessible name, `main`. A "Skip to content" link is the
  first focusable element ([2.4.1 Bypass Blocks](https://www.w3.org/TR/WCAG22/#bypass-blocks)).
- **Wide (min-width 960px)**: nav as a fixed column of 224px on the left; main content max 880px wide.
- **Narrow (below 960px)**: the nav collapses behind a **Menu** button in the header
  (`aria-expanded`, `aria-controls`); the menu closes on a link, on Escape (focus back to the button).
- **Forms**: from 720px up, the label in a column of 200px and the control beside it; below, the label
  above the control. Hints and errors sit under the control.
- Works at 320 CSS px wide and at 400% zoom with no horizontal scrolling, except the content of a
  code block ([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)); text resizable to 200%
  ([1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)); nothing depends on orientation
  ([1.3.4](https://www.w3.org/TR/WCAG22/#orientation)).
- **Sticky save bar**: the scroll container sets `scroll-padding-bottom` to the bar's rendered height
  plus `--space-3`, kept current as the bar grows (an alert and its button can triple it at phone
  width: 242 px measured at 375 px in the 1b review), so a focused field is never hidden behind it; a
  fixed guess does not do. Below a viewport height of 32rem (a phone in landscape, 400% zoom) the bar
  is not sticky and sits at the end of the form
  ([2.4.11 Focus Not Obscured (Minimum)](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum),
  [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).
- **Field grid**: from 720px up, everything of a field but its label sits in the control column:
  the control, the hint and the error.
- `<html lang="en">` ([3.1.1](https://www.w3.org/TR/WCAG22/#language-of-page)).

## 5. Visual system

### 5.1 Tokens

CSS custom properties on `:root`, dark values under `@media (prefers-color-scheme: dark)`. No theme
switch **(preference)**: the system setting decides. Contrast figures are against `--bg` / `--surface`,
computed with the WCAG relative-luminance formula; the floors are 4.5:1 for text
([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)) and 3:1 for control borders, focus rings
and state shapes ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)).

| Token | Light | Dark | Use | Contrast light / dark (bg, surface) |
|---|---|---|---|---|
| `--bg` | `#ffffff` | `#0f1115` | page | |
| `--surface` | `#f6f7f9` | `#181b22` | cards, header, save bar | |
| `--text` | `#1b1f24` | `#e6e8eb` | body text | 16.6, 15.5 / 15.4, 14.0 |
| `--muted` | `#59636e` | `#9ba5b0` | hints, secondary text, values of disabled controls | 6.1, 5.7 / 7.6, 6.9 |
| `--control` | `#818b98` | `#7d8794` | input and checkbox borders, dividers that carry meaning | 3.5, 3.2 / 5.2, 4.7 |
| `--accent` | `#0a5cc2` | `#6cb0ff` | links, primary button, focus ring, selected nav | 6.3, 5.9 / 8.4, 7.6 |
| `--on-accent` | `#ffffff` | `#0f1115` | text on `--accent` | 6.3 / 8.4 |
| `--danger` | `#b42318` | `#ff8d80` | errors, Failed | 6.6, 6.1 / 8.4, 7.7 |
| `--success` | `#1a7f37` | `#56c271` | Running, Saved | 5.1, 4.7 / 8.4, 7.7 |
| `--warning` | `#8a5a00` | `#d9a72e` | Starting, Stopping, "restart to apply" | 5.9, 5.5 / 8.6, 7.8 |

- **Type**: the system font stack (`system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`);
  monospace `ui-monospace, "Cascadia Mono", Menlo, monospace` for paths, flags and values. Sizes in
  `rem`: 0.875 (hints, badges), 1 (body), 1.25 (h2), 1.5 (h1); line height 1.5.
- **Spacing**: 4, 8, 12, 16, 24, 32 px (`--space-1` to `--space-6`). Radius 6px. Focus ring:
  2px solid `--accent`, offset 2px, on `:focus-visible`
  ([2.4.7 Focus Visible](https://www.w3.org/TR/WCAG22/#focus-visible)).
- **Targets**: every button, checkbox and link at least 24 by 24 CSS px, including its padding
  ([2.5.8 Target Size (Minimum)](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
- **Motion**: none needed; a spinner, if any, stops under `prefers-reduced-motion: reduce`.

### 5.2 Components

Each is one React component, reused by every page.

- **Card**: a `section` with an `h3` heading (the GUI's `SectionCard`); optional intro paragraph in
  `--muted`.
- **Field**: one setting.
  - One control and no text of its own: a `<label>` with the field's label, the control, then the
    hint and the error.
  - Several controls, or a checkbox with its own text: a `<fieldset>` whose `<legend>` is the field's
    label; the main text input is also named by the legend (`aria-labelledby`). So a test finds
    `getByRole('group', { name: 'Port' })` and inside it the checkbox `default`.
  - **Default box** (the GUI's `DefaultLineEdit`): a checkbox `default` (or `auto` for thread counts,
    `No key` for the API key) before the input. Ticked: the input is disabled and shows the default
    value the API gave; unticked: the input is editable and shows again the text it held before it
    was ticked (the default shown, for a field loaded on default), so ticking by mistake loses
    nothing **(preference)**. The key is then written; ticked, it is left unset.
  - **Hint** (the GUI's hint tooltip): hidden behind a toggle button after the label, visible text
    "About", accessible name "About `<label>`" (the label in a visually hidden span), `aria-expanded`,
    `aria-controls` the hint. Each `\n` of the hint text starts a new paragraph. A tooltip is not used:
    it fails on touch and needs 1.4.13's rules **(preference)**.
  - **Error**: a paragraph under the control in `--danger`, starting with an error icon that has no
    text of its own; the control gets `aria-invalid="true"` and `aria-describedby` the error
    ([3.3.1 Error Identification](https://www.w3.org/TR/WCAG22/#error-identification),
    [3.3.3 Error Suggestion](https://www.w3.org/TR/WCAG22/#error-suggestion)). Focus on an error
    goes to the control; when it is disabled (a dependent field, or a default box ticked), the text and
    `aria-invalid` stay on it and focus goes to the field's first enabled control, else to the switch
    that enables the field ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
  - **Disabled controls stay readable**: a disabled control still shows a value, so its value is
    drawn in `--muted`, a checkbox included (its box border and its check mark in `--muted`, not the
    browser's faint disabled grey; a read-only form is all disabled controls). WCAG exempts inactive
    controls from [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast); this rule is a
    **(preference)** so the value can be read.
  - **Dependent field** (enabled only when another is on, e.g. Snapshot folder under Save on
    shutdown): disabled, value kept and written, as in the GUI.
- **Text input**: `type="text"`, numbers with `inputmode="numeric"` (no `type="number"`: it accepts
  `1e3` and changes on the wheel **(preference)**). Paths and URLs in monospace.
- **Secret input**: `type="password"`, `autocomplete="off"`, plus a **Show** toggle button
  (`aria-pressed`), disabled while there is nothing to reveal (No key ticked). A secret the API withholds from this user
  (`withheld` in the reply) has no input and no Show: the field says in a sentence that a value is set
  and who can see it, never a mask string standing in for the value (a mask would read as the
  secret's length).
- **Select**: a native `<select>`.
- **Checkbox list**: a `<fieldset>` of native checkboxes (Bind to). Its error sits inside the
  fieldset after the last row; the checked rows (the value) get `aria-invalid` and `aria-describedby`
  the error, and focus goes to the first checked row.
- **Save bar**: sticky at the bottom of a form, always rendered on an editable form. Left: a status
  line (`role="status"`); right: **Discard** and **Save changes**. Both use `aria-disabled="true"`,
  not `disabled`, while there is nothing to save, so focus never drops when the form becomes clean
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)); a click then does nothing. While a save
  runs both are `aria-disabled` (a save cannot be called back); the fields stay editable and what
  is typed meanwhile is never overwritten by the answer. Focus moves to an invalid field only after
  its error is rendered, so it is announced with it.
- **State badge**: the instance state as text plus a shape: filled circle Running (`--success`),
  hollow circle Stopped (`--muted`), half circle Starting / Stopping (`--warning`), cross Failed
  (`--danger`), hollow circle for an unknown state. Never colour alone
  ([1.4.1 Use of Color](https://www.w3.org/TR/WCAG22/#use-of-color)). The shape is `aria-hidden`; the
  text is the state's label (`instance-state-*`).
- **Table**: a native `<table>` with `<th scope="col">` headers; a link in the first cell when the row
  opens a page.
- **Message**: one paragraph in place of content (empty, error, not authorized); `role="alert"` only
  when it appears after a user action, otherwise a plain paragraph.
- **Notice**: a bordered paragraph above a form (e.g. "read-only for your account").
- **Dialog**: native `<dialog>` opened with `showModal()`: focus moves into it, Escape closes it,
  focus returns to the control that opened it; title as `h2`, named by `aria-labelledby`. Every
  dialog has a visible button that does what Escape does (Keep editing, Cancel), so touch and
  pointer users can leave it too
  ([APG modal dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)).
- **Buttons**: primary (filled `--accent`) for the one main action of a view; secondary (outlined
  `--control`) otherwise; danger is not a style of its own **(preference)**: destructive actions ask in a
  dialog instead.

## 6. States every page defines

A brief lists each of these for its page, with the text key; a page cannot ship one undefined.

| State | What shows | Announced |
|---|---|---|
| Loading | `<p role="status">` "Loading…" text of the page (e.g. `server-loading`) in place of the content | yes, politely ([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages)) |
| Empty | a Message saying what is missing and, when one exists, what to do | no |
| Error by key | one Message per error key the route can answer (section 7, "Errors from the API"); a reply with no valid key, a malformed body or a network failure is `failed`. Never the reply's raw body | no on load; `role="alert"` after an action |
| Signed out | the sign-in form in place of the page (section 3) | focus to its heading |
| Not authorized | 403 (the manager refused): read, its Message in place of the content; write, the form-level error in the save bar | after an action |
| Read-only (user) | the form with every control disabled, a Notice above it, no save bar | no |
| Unsaved changes | save bar status "You have unsaved changes."; Save and Discard enabled; leaving asks in a Dialog | the status line |
| Saving | Save shows "Saving…" and is `aria-disabled`; controls stay editable but a second Save is ignored | the status line |
| Saved | status "Saved." or "Saved: restart llama-server to apply." while it runs | the status line |
| Refused by a check | client check: each field's error, focus on the first invalid field, status "Not saved: correct the marked fields."; server refusal: its text beside the field it names, or in the save bar when it names none | `role="alert"` in the save bar |

## 7. Wording

- Every text is a key in `fe/src/strings/en_US.json`, used through `t(key, ...args)`; `{N}`
  placeholders by position. Not text, so not in the file: numbers, units, acronyms, paths, flag and
  INI key names, engine names, values from the server ("Key conventions" in `AGENTS.md`).
- Each key is used by the code and each used key exists (`fe/src/strings.test.ts`), so a slice adds
  exactly the keys it uses.
- English. **No em-dashes**, anywhere: in texts, briefs and this guide (`AGENTS.md`). Use `:`, `;`,
  `,` or parentheses.
- Reuse the GUI's text (`crates/core/strings/en_US.toml`) for the same thing; change it only where it
  is wrong on the web (it names a GUI tab, a local file, the browser's machine) and say so in the brief.
- Key names: `<area>-<thing>` (`server-port`, `server-port-hint`).
- **Errors from the API** arrive as `{"key": "<strings key>", "args": [...]}`, the key reusing the
  configurator's where a check exists (bff contract). The fe shows the text of `api-<key>` filled with
  `args` by position, its English copied from `crates/core/strings/en_US.toml` (or `cli.toml`) so the
  web and the GUI say the same thing. The key picks the message, never the status, except 401
  (sign in). A refusal (422, a check that failed) with a key the fe does not know shows `api-other`
  with the key as its value: better a diagnostic than nothing. Any other reply with an unknown key or
  no valid key, a malformed body or a network failure is `failed`.
- Messages say what happened and what to do, in one or two sentences. Buttons are verbs ("Save
  changes", "Sign in"). Labels in sentence case, as in the GUI.

## 8. Accessibility checklist

WCAG 2.2 level AA is the floor ([WCAG 2.2](https://www.w3.org/TR/WCAG22/)). Every brief maps its
requirements to these.

| Rule | Criterion |
|---|---|
| Every control has a visible label that is also its accessible name, or contains it at the start | [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships), [2.5.3 Label in Name](https://www.w3.org/TR/WCAG22/#label-in-name), [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions), [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value) |
| Headings in order: `h1` page, `h2` page section, `h3` card, `h4` group inside a card | [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships), [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels) |
| Everything works with the keyboard alone, in DOM order, no trap | [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard), [2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order) |
| No single-character shortcuts **(preference: none at all; Ctrl+S is not taken over either)** | [2.1.4](https://www.w3.org/TR/WCAG22/#character-key-shortcuts) |
| Changing a control never submits or navigates by itself (the save bar does) | [3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input) |
| Async results (loading, saved, refused, started) in a live region; errors after an action as `role="alert"` | [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages) |
| Errors named in text beside the field, with a suggestion | [3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion) |
| Contrast and focus per section 5.1; state never by colour alone | [1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color), [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum), [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast), [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible) |
| The nav, header and user menu sit in the same place and order on every page | [3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation), [3.2.4](https://www.w3.org/TR/WCAG22/#consistent-identification) |
| A session that ends keeps the user's edits; nothing asks for the same data twice | [3.3.7 Redundant Entry](https://www.w3.org/TR/WCAG22/#redundant-entry) |

## 9. Tests (for the tester)

- Vitest + Testing Library in jsdom, `fetch` stubbed by URL and method; no bff runs.
- Query by role and accessible name, with names taken from `en_US.json` through `t()` (as
  `fe/src/Instances.test.tsx` does), never by CSS class or test id.
- Failure paths first: each error key, a malformed reply, a network failure.
- Layout, colour and contrast are not tested in jsdom; the designer's review checks them in the code.
