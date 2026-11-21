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
   they appear when their second item or their page does.
5. **Smallest design that does the job.** Reuse a component of section 5 before adding one; add a
   token or a pattern here only when a slice needs it.

## 2. Information architecture

### 2.1 The map

Every page of the finished web UI, in nav order. "Slice" says when it is built; a page with no slice
is not in the nav yet. "Who": "anyone" includes the signed out; "administrator" means signed in, which
only an administrator can be (section 3). Since slice S1a (`fe/briefs/09-signin-only.md`, user
2026-11-19) no page is "anyone": a visitor who has not signed in sees only the sign-in page.

| Nav entry | Route | What it holds | `pai-config` equivalent | Who | Slice |
|---|---|---|---|---|---|
| Overview | `#/` | Framework version; the instances as tiles under the host name (id as a link, state badge, engine when two or more; Port, Tags, Start at boot) | (none; the footer status) | administrator (since S1a) | 1a (moves today's page); H6a, H6b (`fe/briefs/08-hearth-h6-overview.md`); S1 (`fe/briefs/09-signin-only.md`); T1 (`fe/briefs/12-instance-tags.md`); 13 (`fe/briefs/13-node-about.md`) |
| Instances > *instance* | `#/instances/<id>` | Header: name, engine, port, state badge, Start / Stop / Restart, start at boot, Instance tags. Below, the instance's pages as tabs once there are two: **Server** (server file), **Logs** | Server tab, run controls, Settings' "start at boot", View logs | administrator | 1a (read), 1b (save), 1c (run controls); T1 (port, tags); Logs later |
| Profiles > Model profiles | `#/profiles/model`, `#/profiles/model/<id>` (an old `#/models` or `#/models/<rest>` is rewritten in place to `#/profiles/model` or `#/profiles/model/<rest>`, no history entry: `fe/briefs/05b-profiles-nav.md` 5b.8, 5b.9) | The one mixed model profile list (every engine), each row with its engine and on/off state; the model profile editor, its engine the first row of the Assets card (section 2.2) | Models tab | administrator | 2a (list), 2b (filter), 2c (on/off), 2d (model profile page, read), 2e (plain cards, read: `fe/briefs/02e-model-profile-cards.md`), 2h (save of those cards: `fe/briefs/02h-model-profile-save.md`); GD1, GD2 (GPU distribution: `fe/briefs/14-gpu-distribution.md`); the rest `fe/briefs/02-models.md` section 0; 05b (the Profiles group and the routes) |
| About (last entry) | `#/about` | Per host: the framework's version, each installed engine and its GPU backend packages with their versions (`GET /api/node`) | (none; the window title's version) | administrator | 13 (`fe/briefs/13-node-about.md`) |
| Downloads | `#/downloads` | Hugging Face search, candidates, running downloads | HF dialog, Downloads window | administrator | later, **open** (below) |
| Benchmarks | `#/benchmarks` | Run a benchmark, saved runs, reports | Benchmark tab | administrator | later, **open** (below) |
| Results | `#/results` | The benchmark portal: published campaigns from `bench.db`, read-only | (none) | anyone (to decide) | later |
| Hardware | `#/hardware` | Devices, live GPU / CPU / memory / disk monitor of the server machine | HW Monitor | administrator | later |
| Integrations | `#/integrations` | OpenCode and Claude Code snippets built from the server settings | Integrations tab | administrator | later, **open** (below) |
| Gateway | `#/gateway` | The OpenAI-compatible endpoint in front of the instances: keys, routing | (none) | administrator | later |
| Settings | `#/settings` | The web user's own preferences | Settings tab | administrator | later, **open** (below) |

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
- The Overview's tiles show an engine line ("`<engine>` instance") only when the instances span two or
  more engines.
- **Tags** (T1, `fe/briefs/12-instance-tags.md`) are labels only (the router routes by model name).
  A server profile has tags and an instance has its own; the instance shows the union, the
  profile's first. The tile shows the union unlabelled; the instance page edits only the instance's
  own, as Chips (brief 15, user 2026-11-21: group **Instance tags**, each add and removal saved at
  once, outside the Server form; a typed, unconfirmed tag makes leaving ask, as the Server form
  does; the profile's tags are fixed chips under "From the server profile", read-only until the
  server profile page). Tags are data, never translated; the tile joins them by ", " as in the file
  (`Tags = coding, fast`).
- **Model profiles** are one mixed list for every engine. Each model profile has an engine, chosen
  from the **available engines** the bff lists (today only llama.cpp). A model profile's engine is the
  engine whose model profile file holds it (each engine keeps its own files: llama.cpp's `presets.ini` and
  `presets.ini.disabled`), never a key in the file: llama-server refuses a `--models-preset` file with
  a key it does not know (llama.cpp v1.0.0 `common/preset.cpp`, "option not recognized in preset").
  - The model profile editor shows the **Engine** as text, the first row of the Assets card (H11b,
    user, 2026-11-18: `fe/briefs/08-hearth-h10-h11.md` section 3), in the body font, not mono, and
    in the summary line under the title. There is no engine control: moving a model profile to
    another engine's file, which writes no key, is a later slice with its own brief.
  - Every model profile today is in llama.cpp's files, so it is a llama.cpp model profile: the editor shows llama.cpp,
    no migration prompt.
  - **Engine not available**: a model profile in the file of an engine the list lacks (e.g. Strata
    uninstalled); possible only once a second engine's model profile file exists. The
    list row shows the engine name with the text "not available"; the editor's Engine row reads
    "`<engine>` (not available)" and a Notice (icon, no role) above the Assets card says no instance
    can serve the model profile until the engine is installed again, so it is seen with the card closed. Its other fields stay readable;
    fields of an engine the fe does not know are shown read-only as raw `key = value` lines, kept on
    save.
  - The model profile list's Engine column is always shown (each row has an engine), sorted with the list.
- Engine names are data (`llama.cpp`, `Strata`): never in the strings file, never translated.

### 2.3 Navigation

- Hash routes (`#/...`): the bff serves only `/` and `/assets/*`, so the fe routes on the fragment
  and the bff needs no fallback route.
- The nav lists only pages that exist. Under the host's name one link per instance, from
  `GET /api/instances`, in the manager's order: the group label is the `name` of `GET /api/node`, read
  once per session by the shell, and "Instances" while that read is pending or after it failed (the
  shell's read never signs out); the Overview's `h2` over the tiles follows the same rule. Hub mode
  (step 14) repeats the group per node, each label then a disclosure button; with one host there is
  no disclosure (principle 4; `fe/briefs/13-node-about.md` decisions 1 to 3). Signed in, after it (or right after Overview when the
  instances read is loading, failed or empty), the "Profiles" group: a `p.nav-label` and a `ul`
  labelled by it, today with the one link "Model profiles"; "Server profiles" joins it when its page
  exists (05c). Last, in a `ul` of its own, **About** (`info` icon; brief 13). Each group label is text, not a link. Signed out, or while the session is read, there
  is no nav and no Menu button, and `main` spans the whole `.layout` row from 960 px up
  (`.layout > main:only-child`; `fe/briefs/09-signin-only.md` S1a.4, S1a.12).
- The current page's nav link has `aria-current="page"`. On a page below a nav entry that has no
  link of its own (a model profile's page under Model profiles), that entry keeps the current-page style with
  `aria-current="true"`: it leads elsewhere, but the page belongs to it
  ([WAI-ARIA 1.2, `aria-current`](https://www.w3.org/TR/wai-aria-1.2/#aria-current)). Such a page
  also starts with a link back to its list, before the `h1` (`fe/briefs/02-models.md` 61).
- The current-page style (`fe/briefs/07-hearth-h5-shell.md` H5b.3): `--accent-soft` fill, `--accent`
  text, weight 600 (drawn with the 700 face, the only bold one shipped), no inset bar. The fill is
  decoration (1.09:1 light, 1.35:1 dark on `--page`); the non-colour cue is the weight
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color); bold is a cue in
  [G182](https://www.w3.org/WAI/WCAG22/Techniques/general/G182)). Under forced colours the
  current entry takes `Highlight` / `HighlightText` with
  `forced-color-adjust: none`, as the pressed theme switch does (H5c.6; the focus ring stays seen on
  it: H5c review, Chromium and Firefox). Text on the fill 6.04:1 light, 6.93:1 dark.
- On every route change: `document.title` becomes `<page> | Private AI Framework`
  ([2.4.2 Page Titled](https://www.w3.org/TR/WCAG22/#page-titled)), and focus moves to the page's
  `h1` (`tabindex="-1"`), so a screen reader announces the new page
  ([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)).
- Leaving a page with unsaved changes asks first (section 6, "Unsaved changes"), whether by a link,
  the browser's Back or Forward, or a hash typed in the address bar (the app puts the address back
  and asks: `fe/briefs/01-server.md` 38b); closing the tab triggers the browser's own
  `beforeunload` prompt.

## 3. Who is acting: sign-in

The user signs in with their OS account; the bff forwards the identity to `private-ai-manager`,
which applies its rules (user decision, 2026-11-14; `AGENTS.md` and `PLAN.md` step 10).

**Administrators only** (user, 2026-11-15): the web UI is run by administrators. An administrator is
a member of the OS group `pai` on the server machine (created empty by the packages, on Linux and
Windows; not `sudo`, `wheel` or Administrators). Sign-in succeeds only for a member; anyone else is
refused at sign-in with a message (`fe/briefs/01-server.md` 14b).
So there is one signed-in role, and no page has a read-only view for a lesser one.

| State | How the fe knows | Sees | Can do |
|---|---|---|---|
| Signed out | `GET /api/session` answers 401 | the sign-in page only, at every address; no nav, no Menu button, no Sign in in the header (the banner holds the home link and the theme switch) | sign in; the fe reads only the session (`fe/briefs/09-signin-only.md` S1a.1 to S1a.4) |
| Signed in (an administrator) | `GET /api/session` answers 200 `{"user"}` | every page, editable, with the save bar | start, stop, restart, save files, set start at boot |

- The fe does not read a role from the session. The manager still decides each request; a refusal
  that keeps the session is shown as `not-authorized` wherever it lands.
- **Removed from the group after signing in** (user, 2026-11-16): the bff ends the session and says
  so with 403 `session-not-administrator` (`fe/briefs/01-server.md` S1). Where an action or edits
  are at stake the page takes the session-ended state below with the message `session-revoked`; on
  a read the app signs out, the sign-in form showing that message.
- A signed-out user who opens any address sees the sign-in page in its place, the address kept, and
  lands on that page after signing in, with no history entry added; the notice "Sign in to see this
  page." shows only at an address other than `#/`, an empty hash and `#/sign-in`. Signed in,
  `#/sign-in` is an alias of `#/`, rewritten by `history.replaceState`
  (`fe/briefs/09-signin-only.md` S1a.7 to S1a.10).
- The session can end while a page is open. When a page's own **read** answers 401 (the
  Overview's version and instances reads, About's node read, the instance page's first list read and its
  server read, the model profile list and one profile), the app signs out: the sign-in page at the
  same address, the alert "Your session has ended. Sign in again to continue."
  (`signin-session-ended`, in the place of "Sign in to see this page."; the revoked alert wins over
  it), the focus on its `h1`; after a sign-in the page reads again (`fe/briefs/09-signin-only.md`
  S1b). On the instance page neither the polled list nor the boot read signs out (a sign-out there
  would drop unsaved Server edits with no question): the polls change nothing, the boot field shows
  `boot-load-failed`, and the next save or action shows its own 401 handling (below).
- When an **action or a save** answers 401, the page keeps what it shows and any unsaved edits,
  the header and nav stay as they were, and the page offers **Sign in** in a dialog; after it, the user retries. The same user keeps the page as it was, focus
  on the control whose request was refused. Another user makes the page read its data again for the
  new session; unsaved edits are dropped (they belong to the session that made them) and an alert
  says so.
- Sign out is beside the user name in the header; with unsaved changes it asks first.
- The sign-in form lets password managers and paste work: `autocomplete="username"` and
  `"current-password"`, no paste blocking, no puzzle
  ([3.3.8 Accessible Authentication (Minimum)](https://www.w3.org/TR/WCAG22/#accessible-authentication-minimum)).
- **Plain HTTP on the LAN** (user, 2026-11-17): signing in is never blocked, but when
  `window.isSecureContext` is `false` (neither HTTPS nor a loopback address) the sign-in form, on
  its page and in the dialog, shows a static Notice before the fields saying the password is sent
  unencrypted, described on the password field; nowhere else (`fe/briefs/03-insecure-sign-in.md`).
- **The sign-in page** (H7, `fe/briefs/08-hearth-h7-signin.md`; the dialog has none of it): a
  decorative drawing (`div.signin-art`, `aria-hidden`, colours by class from `--art-*`, `--text`,
  `--accent`, `--success`) beside a `div.signin-panel` (`--bg`, the card's radius and shadow, a
  transparent 1px border that forced colours draw) holding the heading and the form. Two equal
  columns when `main` has room for two 18rem columns (960 px and up as built), stacked with the
  drawing first otherwise; the drawing is hidden below 480 px and in forced colours, where the
  panel takes the whole width. The pair (880 px at most, as every child of `main`) is centred in
  `main` (`margin-inline: auto`, brief 10 R1.7; a preference). Focus, reading order and Tab order are the form's: the drawing is
  absent from the accessibility tree ([1.1.1](https://www.w3.org/TR/WCAG22/#non-text-content)).

## 4. Layout and responsive rules

- **App shell**: a header (the mark and the product name as the home link, the Menu button when
  narrow, the theme button, then the user name and Sign out, or Sign in), a nav, and the main region.
  The theme button (`fe/src/ThemeSwitch.tsx`, `fe/briefs/07-hearth-h4-theme.md`): a secondary
  `button` with `aria-pressed` (true while the dark theme is in effect), named "Dark theme" in both
  states ([APG, Button](https://www.w3.org/WAI/ARIA/apg/patterns/button/)), Lucide's moon before the
  label. It shows in every session state. The choice is `localStorage` `pai-theme` (`light` or
  `dark`), mirrored by `data-theme` on `<html>` only while it is stored; a press back to the system's
  theme drops it, and the page follows the OS again (live, `matchMedia` change). Pressed look:
  `--accent-soft` fill, `--accent` text and border; under forced colours `Highlight` /
  `HighlightText`. The stored choice is applied in `main.tsx` before the first render: measured
  2026-11-18 on the stub, one frame of the empty page's background in the OS's theme may paint
  first (Chromium, about 6 ms; none seen in WebKit), and as long as the script takes to arrive on a
  slow link (300 ms delay: 300 ms in Chromium and Firefox): accepted (brief 07 H4, section 0).
  The mark (`fe/src/Mark.tsx`, `fe/briefs/07-hearth-h3-mark.md`): one `svg.mark`, 2rem, in
  `--accent`, `aria-hidden`, so the link's name stays "Private AI Framework"; the same path is the
  favicon, an inline SVG data URI whose fill follows the browser's scheme. Under forced colours an
  SVG keeps its author `color` (the UA's `svg { forced-color-adjust: preserve-parent-color }`,
  [CSS Color Adjustment 1, 3.2](https://www.w3.org/TR/css-color-adjust-1/#forced-color-adjust-prop)),
  so `.mark` needs `color: inherit` there to take the link's system colour (H3.10).
  Icons (`fe/src/Icon.tsx`, `fe/briefs/08-hearth-h8-icons.md`): one component, `Icon name=...`, the
  path data of `lucide-static` 1.74.0 (ISC) in an `svg.icon` with `stroke="currentColor"`,
  `aria-hidden`, `focusable="false"`, no size attribute; `.icon` is 1.25em square, `flex: none`, and
  sets no colour, so it takes its host's text colour in every state and the system colour under
  forced colours. Every icon is decorative, the first child of a host that holds the same words as
  text, at `--space-2` from them. In the shell: Menu (`menu`), the theme button (`moon`, kept at
  1.25rem by `.theme-switch svg`), Sign in (`log-in`), Sign out (`log-out`), the nav's Overview
  (`house`), Model profiles (`layers`) and About (`info`, brief 13); instance links get none (their ids are data). The icon
  hosts are `.menu-button` (a class: an id would outweigh the wide screen's hide rule) and
  `.session button` as `inline-flex`; a nav entry with an icon wraps its label whole under the icon
  rather than mid-word (`:has(> .icon)`, brief 08 H8a.5). Measured 2026-11-19: 21.3 px beside the
  17 px body text (the moon 20), centre 1.3 to 1.5 px above the text's x-height centre, the banner's
  height unchanged.
  In the pages (brief 08 H8b): Start, Stop, Restart (`play`, `square`, `rotate-cw`; `.run-actions
  button` as `inline-flex`, the icon greyed with the text of a switched-off verb), the model profile's
  back link (`chevron-left`, `inline-flex`), the Notices (`info` for "sign in to see this page",
  `triangle-alert` for the revoked session and the engine Notice, `lock-open` for the plain-HTTP
  one), every field error (`circle-alert`, inline, `vertical-align: -0.25em`, 17.5 px beside the
  small text) and the Model profiles filter: `search` in `--muted` (system colour under forced
  colours), absolutely placed `--space-3` inside the field's left edge with `pointer-events: none`,
  the field's left padding widened so the typed text starts 9 px past the icon at any text size. The
  filter icon is decorative (the field has its visible label), so 1.4.11 does not ask a contrast of
  it; it has 6.2:1 (light) and 7.5:1 (dark) on the field anyway. At 320 px the three verbs, now 99
  to 117 px wide, wrap to two rows (Start and Stop, then Restart): 88 px high, 40 before. Measured
  2026-11-19 in Chromium, Firefox and WebKit.
  The banner (`fe/briefs/07-hearth-h5-shell.md` H5a) sits on `--page` with no fill and no rule; Menu,
  the theme button and the session part are one plain `div.banner-tools` (no role) pushed to the end.
  Below 480 px the theme button is icon-only (40 by 40, name still "Dark theme") and "Signed in as
  ..." is visually hidden (still read); the Sign in link is a pill at least 32 px high (gone from slice
  S1: signed out the tools hold the theme button only). Measured
  2026-11-18, signed in: 72 px (one row) at 1280, 112 px (two rows) at 479 and 320 px.
  The nav (H5b) has no fill either and no rule on its right: at 960 px and up the column ends at
  x 224 and the main region's content starts 40 px further, the gap is the only divider. There the
  nav has no right padding (08 H8a.10), so the pills end at x 224 and the current, bold "Model
  profiles" keeps its label beside its icon at 100 % text (34 px, about 9 px of slack, measured
  2026-11-19); the 3 px focus ring with its 2 px offset reaches x 229, into `main`'s padding, and
  nothing clips it. Entries are
  pills (`--radius-pill`, at least 32 px high, measured 34; a long id wraps inside, two lines 61 px
  at 320, three 87 px in the 224 px column); group labels in `--fs-xs` `--muted` (5.65 / 7.52) and
  each labelled list indented beside a decorative 2 px `--line` rule; below 960 px the open menu ends
  with a 1 px `--line` rule.
  Landmarks: `header` (banner), `nav` with an accessible name, `main`. A "Skip to content" link is the
  first focusable element ([2.4.1 Bypass Blocks](https://www.w3.org/TR/WCAG22/#bypass-blocks)).
- **Wide (min-width 960px)**: nav as a fixed column of 224px on the left; main content max 880px wide.
- **Narrow (below 960px)**: the nav collapses behind a **Menu** button in the header
  (`aria-expanded`, `aria-controls`); the menu closes on a link, on Escape (focus back to the button).
- **Forms**: from 720px up, the label in a column of 200px and the control beside it; below, the label
  above the control. Hints and errors sit under the control.
- Works at 320 CSS px wide and at 400% zoom with no horizontal scrolling, except the content of a
  code block and a data table, which scrolls inside its own box (Table, 5.2)
  ([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)); text resizable to 200%
  ([1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)), and the page still reflows at 320 px with
  200% text, as the Understanding page of Reflow asks
  ([Understanding 1.4.10](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html)); nothing depends
  on orientation ([1.3.4](https://www.w3.org/TR/WCAG22/#orientation)). A word longer than its line
  breaks (`body { overflow-wrap: break-word }`, inherited); a flex item that holds text needs
  `anywhere`, since `break-word` leaves its minimum width at the longest word
  ([CSS Text 3](https://www.w3.org/TR/css-text-3/#overflow-wrap-property); brief 10 R1.1, R1.2).
- **Sticky save bar**: the scroll container sets `scroll-padding-bottom` to the bar's rendered height
  plus `--space-3`, kept current as the bar grows (an alert and its button can triple it at phone
  width: 242 px measured at 375 px in the 1b review), so a focused field is never hidden behind it; a
  fixed guess does not do. The bar's own buttons cancel that padding with an equal negative
  `scroll-margin-bottom`, so focusing Discard or Save does not scroll the page (without it, measured
  in the H5c review: Chromium 430 px, WebKit 112 px on each focus; with it 0 in all three engines).
  Below a viewport height of 32rem (a phone in landscape, 400% zoom, or 200% text on a 1023 px high
  window: `rem` in a media query is the browser's text size) the bar is not sticky, both rules are 0,
  and the bar sits at the end of the form
  ([2.4.11 Focus Not Obscured (Minimum)](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum),
  [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).
- **Field grid**: from 720px up, everything of a field but its label sits in the control column:
  the control, the hint and the error.
- `<html lang="en">` ([3.1.1](https://www.w3.org/TR/WCAG22/#language-of-page)).

## 5. Visual system

### 5.1 Tokens

CSS custom properties on `:root`, the 16 dark values twice, identical (pinned by a test): under
`@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) }` (the system's dark, no
script needed) and under `:root[data-theme='dark']` (a stored dark choice) with `color-scheme: dark`;
`:root[data-theme='light']` sets only `color-scheme: light`, so native controls and scrollbars follow
the theme in effect (brief 07 H4 section 1; measured 2026-11-18: every `:root` custom property
computes identical between a forced theme and the system's same theme, in the three engines). The palette is direction B "Hearth" (brief 04,
round 2; built by brief 06 H1a, 2026-11-18): warm paper behind everything, cards lifted off it. Every
foreground and background pair, both themes, with its measured ratio and floor, is the table of brief
`fe/briefs/06-hearth-h1-tokens.md` section 2, pinned by `HearthTokens.test.ts`; the floors are 4.5:1 for
text ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)) and 3:1 for control borders, focus
rings and state shapes ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)). No colour literal
outside the two token blocks, except `.dialog::backdrop` (custom properties do not reach `::backdrop`).
A text colour on a surface outside that table needs a new row first.

| Token | Light | Dark | Use | Lowest ratio light / dark (on the surfaces it is used on) |
|---|---|---|---|---|
| `--page` | `#f7f1e8` | `#1b1714` | behind everything (`body`) | |
| `--bg` | `#fffdf9` | `#231d19` | cards, save bar, dialog, secondary and inactive buttons | |
| `--surface` | `#f5efe6` | `#2b241f` | nav (until H5b), notices, fill of a disabled control | |
| `--field` | `#fffdf9` | `#1b1714` | fill of an enabled input, select and unchecked checkbox | |
| `--line` | `#e6dccf` | `#3a3029` | decorative dividers only (table rows): carries no meaning | none (no floor) |
| `--accent-soft` | `#f1e4f6` | `#3a2a44` | About open; later the current nav entry and the Running badge | |
| `--text` | `#2a211c` | `#f1e9df` | body text, headings | 13.78 / 12.70 |
| `--muted` | `#6b5d52` | `#b5a698` | hints, secondary text, values of disabled controls, placeholder | 5.55 / 6.45 |
| `--control` | `#8c7d70` | `#8a7c6f` | control borders (dashed on a disabled control), dividers that carry meaning | 3.48 / 3.78 |
| `--accent` | `#7b3a8f` | `#dcaaf0` | links, primary button, focus ring, selected nav, checked box | 6.04 / 6.93 |
| `--on-accent` | `#ffffff` | `#1b1714` | text and tick on `--accent` | 7.38 / 9.37 |
| `--danger` | `#b3261e` | `#ff9a8a` | errors, Failed | 5.72 / 7.45 |
| `--success` | `#2f6f3e` | `#7ccf8e` | Running, Saved | 5.30 / 8.13 (4.96 light on `--accent-soft`, unused until H6) |
| `--warning` | `#8a5300` | `#e8b45a` | Starting, Stopping, "restart to apply" | 5.54 / 8.08 |

Shape tokens: `--control-h` 40px (buttons and fields, a `min-height`), `--border-w` 1.5px (control
borders; at a device pixel ratio of 1 the browsers draw it 1 px, since border widths snap down to
whole device pixels: [CSS Values 4](https://www.w3.org/TR/css-values-4/#lengths)), `--ring-w` 3px,
`--hatch` (the disabled stripes), `--radius-check` (checkbox), `--radius-pill` (primary, secondary,
About buttons, switch track), all in use since H2a; `--radius-card` 16px and `--shadow` (redeclared
in dark as a 1 px `--line` outline) on the card, the table box (`.table-scroll` from brief 10), the save bar (top corners only) and
the dialog since H2b, and on the sign-in page's panel since H7; the drawing's own colours
`--art-1` (`#ead9f0` / `#3a2a44`), `--art-2` (`#f3e3cf` / `#3b2f24`, its box) and `--art-3`
(`#ffffff` / `#2b241f`, the door), decoration only, in no contrast pair (H7); declared for later slices: `--space-7` 64px (brief 06 section 1). The lift is
decorative (cards carry no meaning): in light the card is told from the page by the shadow and the
fill (`--bg` on `--page` 1.11:1), in dark by the outline (1.39:1 on `--page`) and the fill (1.07:1),
measured in the H2b review; no floor applies. The card and the table box carry a transparent 1 px
border, the edge that shows in forced colours, where a shadow computes to none. In light `--bg` and `--field` are the same
colour; in dark an enabled field is darker than its card.

- **Type**: Atkinson Hyperlegible Next (400 and 700) for text, Atkinson Hyperlegible Mono (400) for
  paths, flags and values: six WOFF2 files (latin and latin-ext) from `@fontsource` 5.3.0, bundled in
  `fe/dist/assets/`, never fetched from another origin, `font-display: swap`; each stack goes on to a
  metric fallback face (Arial or Liberation Sans, Courier New or Liberation Mono, with `size-adjust`
  and ascent/descent overrides: brief 06 section 9) and then the system stacks of before. A page
  fetches only the latin files it uses (a latin-ext file only for a letter outside latin); a weight
  of 600 renders with the 700 face; there is no italic face and the UI sets no italic; arrows
  (U+2190, U+2192) are not in the fonts and would fall back. Every `font-size` is a token: `--fs-xs` 0.8125rem,
  `--fs-sm` 0.875rem (hints, notes, full-value line, About, field error, nav label), `--fs-md`
  0.9375rem, `--fs-body` 1.0625rem (17 px), `--fs-lg` 1.125rem (h3), `--fs-h2` 1.375rem, `--fs-h1`
  2rem (line height 1.2); body line height `--lh` 1.55. Bold is 600.
- **Spacing**: 4, 8, 12, 16, 24, 40, 64 px (`--space-1` to `--space-7`). Radius `--radius` 10px
  (fields, notices, alerts; cards, tables, the save bar and the dialog use `--radius-card`); buttons are pills. Focus ring: 3px
  (`--ring-w`) solid `--accent`, offset 2px, on `:focus-visible`
  ([2.4.7 Focus Visible](https://www.w3.org/TR/WCAG22/#focus-visible)); measured at least 6.04:1
  light and 6.93:1 dark against every colour beside it (the About toggle open, on `--accent-soft`;
  H1a review; with the 3 px ring 6.58:1 light and 8.76:1 dark at the lowest, nothing clips it, H2a
  review).
- **Targets**: every button, checkbox and link at least 24 by 24 CSS px, including its padding
  ([2.5.8 Target Size (Minimum)](https://www.w3.org/TR/WCAG22/#target-size-minimum)). Buttons and
  fields are at least 40 px high (`--control-h`), the jump list's buttons 32 px, About 24 px, a Chip
  32 px with its Remove button 24 px, the chips' + and the pill's Add and Cancel 32 px; the
  checkbox box is 20 px (1.25rem) inside a `.check` label of at least 24 px that activates it, with no
  other target within the 24 px circle (2.5.8's spacing exception); the switch track 44 by 24 px.
- **Motion**: none needed; a spinner, if any, stops under `prefers-reduced-motion: reduce`.

### 5.2 Components

Each is one React component, reused by every page.

- **Card**: a `section` with an `h3` heading (the GUI's `SectionCard`); optional intro paragraph in
  `--muted`. A note between fields (the GUI's muted text) is drawn as the intro, at its place. A
  named group inside a card is one `div.subgroup` holding its `h4` and then its fields; when fields
  in two groups share a label, the fields sit in a `<div role="group">` named by the `h4`
  (`aria-labelledby`), the `h4` and that div the subgroup's only two children
  (`fe/briefs/02e-model-profile-cards.md` 90, 90a). A subgroup has `--space-5` above it, more than the
  24 px between two fields, so a group heading sits with the fields under it; a subgroup right
  after the card's `h3` has none (the `h3`'s bottom margin parts them).
  - **Collapsed card** (the model profile page's Assets, 08 H11a): a card whose rows are reference,
    not settings, may close behind a disclosure in its own heading: the `h3` (`tabindex="-1"`, the
    jump target) holds only a `button` (`aria-expanded`, `aria-controls` the body) with the card's
    title and a `chevron-down` at the far end, turned 180 degrees when open, no transition; the
    button has no border or fill, the heading's font, full width, at least 32 px high. The intro
    stays outside the body, so the closed card still says why it reads only; the body is `hidden`
    when closed (out of the tab order and the accessibility tree) and holds nothing focusable.
    Closed on every arrival, no storage; Save and Discard leave it as it is; a jump to it opens it
    in the same commit as the list's close. Read as "Assets, heading level 3", "Assets, button,
    collapsed" ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). Under forced colours it
    reads as a control by its text and chevron, no border.
  - **Summary line** (08 H11a.1): right under the model profile page's `h1`, a plain `p` in `--muted`
    saying the engine and the model file (the last non-empty segment of the path, split on `/` and
    `\`), or "no model file"; wrapping anywhere, so the closed Assets card still names the file.
- **Read-only form**: a form shown before its slice can save (the model profile page's plain cards, 2e):
  the real controls, each with the native `disabled` attribute, under one Notice saying why, so the
  saving slice only enables them (2h does, card by card). Not `readonly` (text inputs only in HTML) and not `aria-disabled`
  (dozens of inert tab stops). The About toggles stay operable.
  - **Full-value line**: a disabled input or select cuts off a long value and cannot be scrolled,
    so a value longer than its control shows (more than 8 characters in a number input, more than
    20 in a text input or a select) is repeated in full under the control: `--muted`, monospace,
    0.875rem, wrapping anywhere, `aria-hidden="true"` (the control already exposes the value) and
    no tab stop. A count of characters, not a measure of the box, so it is the same at every width
    and testable without layout **(preference)**. Brief 02e, criteria 118 to 121. Only under a
    **disabled** control: an enabled input scrolls and an enabled select opens (02h, 134).
  - **A boolean shows what the engine will do**: a llama-server flag is off only for `off`,
    `disabled`, `false`, `0` and on for any other value, `""` included; a value other than `true`
    or `false` adds a "Written as ..." line (not `aria-hidden`, named by the control's
    `aria-describedby`) so the file's spelling stays visible; a value shown as a select's extra
    option gets no such line, only the full-value line when it is long. Values are trimmed of
    spaces and tabs only, as llama-server's INI grammar does. Each key's reading (flag, truthy
    only, valued) is in brief 02e, criteria 122 to 126. In an editable form the line shows only
    while the field is unchanged (02h, 134).
- **Units** (MiB) sit after the input, never in the label, even where the GUI's label has them. The
  unit has an id and is the first id of the input's `aria-describedby`, so it is read with the
  input ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships); 02h, 159).
- **Changed field and partial save** (a file the user may have hand-edited, the model profiles): a field
  is changed when its control's state differs from the state it loaded in, so changing it back
  makes it unchanged; only changed fields are sent, each in the configurator's spelling, and the
  bff edits those lines in place, so an untouched value keeps its spelling, its comments and its
  neighbours (02h, D-a, 132, 133). The Server page sends its whole document instead, as the GUI
  renders that file whole.
- **Reason line**: a group or card disabled by a rule whose switch is not next to it (a value set in
  another card) shows one `--muted` paragraph saying what it needs, right after its heading, only
  while disabled; a plain paragraph in reading order, not a live region and not in any
  `aria-describedby` (02h, 176 to 181, 192). A dependent field whose switch sits just above it needs
  none (Snapshot folder, Vision GPU).
- **Section jump list**: on a form taller than a few screens (the model profile page), a `<nav>` after the
  form's `h2` holds one button per card; a button moves focus to the card's `h3` (`tabindex="-1"`)
  and scrolls it into view. Buttons, not `#fragment` links, which the hash router would read as a
  route (02h, 196). Drawn as an index, not a toolbar (08 H9a): the buttons carry no class; the `ul`
  is a block column with a decorative 2px `--line` rule at its left; each entry is full width, at
  least 32 px high, no border or fill, `--accent` text (6.58:1 light, 9.37:1 dark) underlined 1px at
  3px offset, no hover change, a long title wrapping inside it; `LinkText` under forced colours. The
  underline and the place in a named `nav` list are what tell an entry from body text, not the
  colour alone ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)). From 1280 px it is a rail
  (08 H9b): the section becomes a grid, the content in a first column, the list in a 15rem second
  column, sticky 16 px from the top, at most the viewport's height minus 32 px and scrolling inside
  beyond that, with 8 px padding at each side so the scroll box does not cut the focus ring. It
  stays first in the DOM, so Tab still reaches it before the cards. The save bar, a grid item of
  the first column, still sticks to the viewport's bottom in the three engines. The nav is named by
  a visible `p` "On this page" (`aria-labelledby`; 08 H10). Below 1280 px that label is hidden and
  the list sits behind a `.secondary` disclosure with the same words and a `chevron-down` after
  them (turned 180 degrees when open, no transition), closed on every arrival; the closed list is
  `display: none` through a `collapsed` class, never `hidden`, so the rail above 1280 px shows it
  whatever the state. A jump closes the list first, then focuses and scrolls to the heading, or the
  vanishing list pulls the heading out of view. No Escape: a disclosure needs none.
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
    text of its own (`Icon circle-alert`, brief 08 H8b.6; a long message wraps under it, inline); the control gets `aria-invalid="true"` and `aria-describedby` the error
    ([3.3.1 Error Identification](https://www.w3.org/TR/WCAG22/#error-identification),
    [3.3.3 Error Suggestion](https://www.w3.org/TR/WCAG22/#error-suggestion)). Focus on an error
    goes to the control; when it is disabled (a dependent field, or a default box ticked), the text and
    `aria-invalid` stay on it and focus goes to the field's first enabled control, else to the switch
    that enables the field ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
  - **Disabled controls stay readable**: a disabled control still shows a value, so its value is
    drawn in `--muted`, a checkbox included (its check mark in `--muted`, not the browser's faint
    disabled grey). WCAG exempts inactive controls from
    [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum) and
    [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast); holding the value to 4.5:1 anyway is a
    **(preference)**, since the read-only cards exist to be read: `--muted` on `--surface` is 5.55:1
    light, 6.45:1 dark.
  - **Disabled looks different from enabled** (2026-11-17, 02h 163a to 163g): every control with the
    native `disabled` attribute (`input`, `select`, `textarea`, a checkbox or switch, a button) is
    drawn **flat**: background `--surface` (slightly darker than the `--bg` card, so it reads as
    part of the card's ground, not a box to type into), border **dashed** `--border-w` `--control`
    (the box stays visible, at 3.48:1 light, 3.78:1 dark on `--surface`), text and check mark `--muted`,
    `cursor: not-allowed`, never `opacity`. A disabled field or button also carries the **hatch**
    (`repeating-linear-gradient(135deg, transparent 0 6px, var(--hatch) 6px 8px)` over `--surface`;
    not on a checkbox or switch, too small to show it): on the darker stripe, as rendered, `--muted`
    text is 4.85:1 light, 5.5:1 dark and the dashed border 3.04:1 light, 3.22:1 dark (H2a review).
    Forced colours drop the gradient; the dashed border stays the cue. An enabled control keeps its
    solid `--control` border on `--field`. The cue is the border's style, not a colour (`--surface` on `--bg` is only 1.13:1
    light, 1.09:1 dark), so it
    holds without colour ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)) and under forced
    colours, which replace border colours but not `border-style`
    ([CSS Color Adjustment 1, properties affected](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)).
    A disabled checkbox's own text (`.check`) is `--muted` too. A ticked default box is enabled, so
    it keeps its filled `--accent` look and its tick, and the input it disables turns flat and
    dashed beside it. A checkbox inactive by `aria-disabled` (a write in flight) shares the drawing
    but keeps `cursor: default` and its label colour: it turns back by itself in a moment. A field's
    own label (outside a checkbox) stays `--text`: it names the setting, the control
    carries the state (user, 2026-11-17). One set of rules in `index.css`, shared by the model profile page and the Server page.
  - **Dependent field** (enabled only when another is on, e.g. Snapshot folder under Save on
    shutdown): disabled, value kept and written, as in the GUI.
- **Text input**: `type="text"`, numbers with `inputmode="numeric"` (no `type="number"`: it accepts
  `1e3` and changes on the wheel **(preference)**), except a number that can be negative, which gets
  no `inputmode`: a phone's numeric keyboard may have no minus key
  ([MDN, `inputmode`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/inputmode)). Paths and URLs in monospace.
- **Secret input**: `type="password"`, `autocomplete="off"`, plus a **Show** toggle button
  (`aria-pressed`), disabled while there is nothing to reveal (No key ticked). A document that
  withholds the secret (`withheld` in the reply) is not shown as a form: the read fails, since a save
  would erase the secret (brief 01, 27d; the "withheld" view for a non-administrator was removed on
  2026-11-15). Never a mask string standing in for a value the page does not have (a mask would read
  as the secret's length).
- **Select**: a native `<select>`. A value from a file that the select does not list is shown as
  one extra, last option whose value and text are the value as written, selected, never silently
  read as the default (as the "(not available)" engine of section 2.2).
- **Switch**: an on/off state written at once and named by a thing rather than a sentence (a
  model profile's row): a native `<input type="checkbox" role="switch">`, state in `checked`, never
  `aria-checked`, made inactive exactly as a checkbox and drawn with the checkbox's fills as a 44 by
  24 px pill track with a round knob: `--control` knob on the left when off, `--on-accent` knob on
  the right of an `--accent` track when on, `--muted` knob on the dashed `--surface` track when
  inactive; the knob jumps, no transition. Under forced colours the knob is `CanvasText` (`GrayText`
  inactive) with `forced-color-adjust: none`, on and off alike, so it never takes the track's colour.
  The word beside it (On / Off) also carries the state
  ([APG Switch](https://www.w3.org/WAI/ARIA/apg/patterns/switch/),
  [ARIA in HTML](https://www.w3.org/TR/html-aria/)). A setting phrased as a statement ("start
  llama-server when the system starts") stays a checkbox.
- **Checkbox list**: a `<fieldset>` of native checkboxes (Bind to). Its error sits inside the
  fieldset after the last row; the checked rows (the value) get `aria-invalid` and `aria-describedby`
  the error, and focus goes to the first checked row.
  - **GPU list** (the model profile page's GPU distribution, brief 14): a Checkbox list of the host's
    GPUs whose rows are `li` of a `ul role="list"`; the label holds the id (monospace, weight 600)
    and the product name, a `--muted` memory line describes the checkbox. A checked row carries its
    own controls after it, in a wrapping flex row: a weight text field (`inputmode="decimal"`,
    about 6 characters) with its share as plain text ("Share: 75%", described by, not live), and
    **reorder buttons**. Checked rows come first, in their saved order; checking appends a row to
    them, and focus stays on the control the user used through any move (rows keyed by id).
  - **Reorder buttons** (wherever order is a value: the GPU list): Move up and Move down, secondary
    buttons in each orderable row, shown only with two or more such rows; the accessible name is
    the visible text plus the row's name in a visually hidden span ("Move up ROCm0",
    [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)); the ends are `aria-disabled`, not
    removed, so focus never drops; the result is said by one visually hidden `role="status"`
    region ("ROCm0: position 1 of 2"). Never a drag handle alone
    ([2.5.7](https://www.w3.org/TR/WCAG22/#dragging-movements)) **(preference: buttons only)**.
- **Chip** (a short list of labels edited one by one: the instance tags, brief 15): a `ul.chips`
  (`role="list"`) of `li.chip` pills in a wrapping flex row (`.chips-row`, gap `--space-2`), each
  the label as text (wrapping anywhere inside the chip) and, when it can be removed, a 24 px
  `.chip-remove` button (icon `x`, name "Remove tag `<label>`"). A fixed chip (not changeable here)
  has no button, `--surface` fill and a transparent border, in its own list named by a visible line.
  After the chips a 32 px round `.tag-add` button (icon `plus`) opens in its place a `.tag-new` pill:
  a one-label input, Add (`check`) and Cancel (`x`); Enter or Add confirms, Escape or Cancel closes,
  blur closes only an empty pill. Each add and removal is written at once (Action feedback, one
  write at a time, controls `aria-disabled` meanwhile); focus after an add returns to the +, after
  a removal to the next chip's button, else the previous, else the +. Suggestions, when the labels
  come from elsewhere, are an APG combobox with a list popup and no automatic selection: the input
  is the `combobox`, the `ul.tag-options` `listbox` sits in the flow under the row (never floating),
  arrows move `aria-activedescendant`, the active option drawn with the inset focus ring; choosing
  one adds it at once ([APG Combobox](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/)). These
  icon-only buttons hold their words in a visually hidden span.
- **Save bar**: sticky at the bottom of a form, always rendered on an editable form. Left: a status
  line (`role="status"`); right: **Discard** and **Save changes**. Both use `aria-disabled="true"`,
  not `disabled`, while there is nothing to save, so focus never drops when the form becomes clean
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)); a click then does nothing. While a save
  runs both are `aria-disabled` (a save cannot be called back); the fields stay editable and what
  is typed meanwhile is never overwritten by the answer. Focus moves to an invalid field only after
  its error is rendered, so it is announced with it. **A form that turns clean keeps no alert
  without a button** (user, 2026-11-17 and, for Discard, 2026-11-18; 02h 198b to 198h): an edit
  or Discard that makes the form clean removes a "Not saved" alert (a check, a refusal, a failure,
  a timeout, a 403 / 404 / 503 text), silently and without moving focus; an alert with its answer
  button (Sign in after a session ended, Reload after a 409) stays, and Reload on a clean form reads
  at once, with no dialog. Field errors keep their own rule (an edit clears that field's; Discard
  clears all).
- **State badge**: the instance state as text plus a shape: filled circle Running (`--success`),
  hollow circle Stopped (`--muted`), half circle Starting / Stopping (`--warning`), cross Failed
  (`--danger`), hollow circle for an unknown state. Never colour alone
  ([1.4.1 Use of Color](https://www.w3.org/TR/WCAG22/#use-of-color)). The shape is `aria-hidden`; the
  text is the state's label (`instance-state-*`). A badge kept current by polling is not a live
  region: changes nobody asked for are shown, not announced; the outcome of the user's own action
  is announced by its status line (Action feedback) **(preference)**. Under forced colours the
  shape opts out of the forced palette (`forced-color-adjust: none`) and paints itself in
  `CanvasText`, so the five shapes stay distinct in the user's own colours
  ([CSS Color Adjustment 1, 3.2](https://www.w3.org/TR/css-color-adjust-1/#forced-color-adjust-prop);
  H6a.10, measured in Chromium emulation and Firefox's forced colours).
- **Tile** (the Overview's instances, H6a and H6b): a `ul.tiles` (`role="list"`: WebKit drops the
  list role of a `ul` with `list-style: none`, [WebKit bug 170179](https://bugs.webkit.org/show_bug.cgi?id=170179)) of `li.tile` cards in a grid
  (three across at 1280, two at 960, one at 480 and below; tiles of a row the same height), in the
  manager's order. Each tile: a head with the id as the link to the instance page (at least 24 px
  each way, [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)) and its State badge, which
  wraps under the id when the row is narrow; the engine line when it tells instances apart; and for
  a signed-in visitor a `dl.tile-facts` as the last child, one `div` per fact (`dt` in `--muted`,
  `dd` at weight 600 with no margin and `overflow-wrap: anywhere` (a flex item: a 32-character tag
  would otherwise push the tile out at 320 px, T1 review), label and value at the two ends, wrapping
  onto two lines when narrow, a 1px `--line` hairline above as decoration). The facts, in order: Port, Tags (T1:
  from the list itself, so shown at once; no row for port 0 or no tags) and Start at boot
  ("On" / "Off", words, never colour alone), from the list's `boot` since slice 13 (no read of its
  own; no row for `null` or anything but a boolean), no `dl` when no fact has a row. Nothing in a tile takes focus but the
  link. Screen readers hear list, item, link, the state, the engine line, then term and definition
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)).
- **Table**: a native `<table>` with `<th scope="col">` headers; a link in the first cell when the row
  opens a page. It is the only child of a `div.table-scroll` (`overflow-x: auto`,
  `scroll-padding-inline: var(--space-2)`), which carries the lift (`--bg`, `--radius-card`,
  `--shadow`, the transparent 1px border): a scroll box clips its child's shadow, so the table has
  none of its own. When the columns do not fit (200% text at 320 px), the table scrolls inside the
  box and the page does not; the column cut at the box's edge shows there is more. The box has no
  `tabindex`, role or name: every row holds a focusable control, and Tab scrolls the box to it
  (brief 10 R1.3 to R1.6,
  [Understanding 1.4.10](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html) on data tables).
- **Value list**: settings shown as text, not as controls (a page or card that reads before its
  slice can write, e.g. the model profile page's Assets in 2d): a `<dl>`, each field a `<dt>` label then
  its `<dd>` value, laid out as Forms in section 4 (label column from 720px). Values from the file
  unchanged, in monospace for paths, flags and values, wrapping anywhere (`overflow-wrap: anywhere`,
  `min-width: 0`); an empty value is the page's "(none)" text in the body font and `--muted`.
  A heading that follows a Value list gets `--space-6` above it (`.assets + h3`, brief 13), so a
  new group never reads as one more row of the list before it. Nothing in it takes focus ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  technique [H40](https://www.w3.org/WAI/WCAG22/Techniques/html/H40)).
- **Message**: one paragraph in place of content (empty, error, not authorized); `role="alert"` only
  when it appears after a user action, otherwise a plain paragraph. A waiting text shown when the
  content first renders and replaced by the outcome (the GPUs card's "Detecting GPUs…", brief 14) is
  a plain paragraph too, not a `role="status"`; a card's own status region (a move's result) is
  always in the DOM, so a page's tests find the save bar's status line by its place in the bar.
- **Notice**: a bordered paragraph above a form, or above a table whose rows hold controls (e.g.
  the "engine not available" Notice of section 2.2). (The read-only Notice for a non-administrator
  was removed on 2026-11-15.) It is a flex row (`align-items: flex-start`, gap `--space-2`): an
  icon first, centred on the first line (`margin-top: calc((1lh - 1.25em) / 2)`), the text wrapping
  beside it, never under it; a word longer than the text column breaks (`overflow-wrap: anywhere`,
  brief 10 R1.2). Since every child is a flex item, a Notice holds the icon and ONE run
  of text: a link, `strong` or button inside it would break the inline flow, so such text goes in a
  `span` (all four Notices of 2026-11-19 hold plain text only).
- **Dialog**: native `<dialog>` opened with `showModal()`: focus moves into it, Escape closes it,
  focus returns to the control that opened it; title as `h2`, named by `aria-labelledby`. Every
  dialog has a visible button that does what Escape does (Keep editing, Cancel), so touch and
  pointer users can leave it too
  ([APG modal dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)).
- **Buttons**: pills, at least 40 px high; primary (filled `--accent`) for the one main action of a
  view; secondary (outlined `--control`, `--border-w`) otherwise; no hover look (brief 06 section 0); danger is not a style of its own **(preference)**: destructive actions ask in a
  dialog instead.
  - **Inactive**: a button that can turn inactive while it has focus (the save bar's, the run
    controls) uses `aria-disabled="true"`, never `disabled`: it stays in the tab sequence, a click,
    Enter or Space does nothing (the handler checks; the attribute blocks nothing by itself), and it
    is drawn with `--bg` background, `--muted` text and `--control` border. Under
    `@media (forced-colors: active)` its text and border are `GrayText`, so it still reads as
    inactive in a high-contrast theme. A checkbox inactive this way (Start at boot while its write
    runs) is drawn as a disabled checkbox (Field, "Disabled controls stay readable"), `GrayText`
    likewise under forced colours. Never by `opacity`: it dims the focus ring with the control,
    below the 3:1 floor of 5.1. (Removed 2026-11-15: "a control never enabled for the role keeps
    `disabled`"; there is one role.) `disabled` stays for a control that cannot turn active while it
    has focus (a dependent field, a ticked default box).
    ([APG, focusability of disabled controls](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#focusabilityofdisabledcontrols),
    [MDN, `aria-disabled`](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Reference/Attributes/aria-disabled);
    `GrayText` is MDN's suggestion there, **(preference)** to adopt it)
- **Action feedback** (an action outside a form: run controls, start at boot): right after the
  control, a status line (`role="status"`, always in the DOM) for progress and outcome, then an alert
  (`role="alert"`, rendered only while it has text) for a failure, then, after a 401, a **Sign in**
  button that opens the sign-in Dialog. A new click on the control clears both. An action always
  ends in its status line or alert, with an outcome, an error or a timeout counted on its own timer
  whatever request is still pending, never a progress text without end. The save bar is the
  same pattern for a form ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)). For a control
  repeated in every row of a table, one region for the table, just above it, whose messages name the
  row; one write at a time, every row's control `aria-disabled` while it runs.

## 6. States every page defines

A brief lists each of these for its page, with the text key; a page cannot ship one undefined.

| State | What shows | Announced |
|---|---|---|
| Loading | `<p role="status">` "Loading…" text of the page (e.g. `server-loading`) in place of the content | yes, politely ([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages)) |
| Empty | a Message saying what is missing and, when one exists, what to do | no |
| Error by key | one Message per error key the route can answer (section 7, "Errors from the API"); a reply with no valid key, a malformed body or a network failure is `failed`. Never the reply's raw body | no on load; `role="alert"` after an action |
| Signed out | the sign-in form in place of the page (section 3) | focus to its heading |
| Not authorized | 403 (the manager refused): read, its Message in place of the content; write, the form-level error in the save bar | after an action |
| Unsaved changes | save bar status "You have unsaved changes."; Save and Discard enabled; leaving asks in a Dialog | the status line |
| Saving | Save shows "Saving…" and is `aria-disabled`; controls stay editable but a second Save is ignored | the status line |
| Saved | status "Saved." or "Saved: restart llama-server to apply." while it runs | the status line |
| Action running | the action's status line ("Starting llama-server…"), its buttons `aria-disabled` until the state settles or a minute passes | the status line |
| Live state | the state badge re-read every 5 s while its page is open; a failed read keeps the last state silently | no (the badge is not a live region) |
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
  no valid key, a malformed body or a network failure is `failed`. Every request has a time
  limit (15 s for a `GET`, 120 s for any other method; reasons in `fe/briefs/01-server.md` 4.8;
  40 s for `GET /api/node/devices`, whose cold probe takes up to 35 s: brief 14 GD1.3): a
  request that runs out is aborted and is that call's network failure, unless its brief says
  otherwise (a save that times out may have been written: 47d there).
- A copied `api-*` text that says what happened but not what to do gets one sentence appended with
  the fix, when the fix is known and the web can do it
  ([3.3.3 Error Suggestion](https://www.w3.org/TR/WCAG22/#error-suggestion)); the brief says so as a
  difference from the GUI (first: `api-model-profile-over-memory`, `fe/briefs/11-resource-refusals.md`).
  Figures from the server are shown as sent, in the unit the API names (MiB), never converted.
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
| The nav, header, user name and Sign out sit in the same place and order on every page | [3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation), [3.2.4](https://www.w3.org/TR/WCAG22/#consistent-identification) |
| A session that ends keeps the user's edits; nothing asks for the same data twice | [3.3.7 Redundant Entry](https://www.w3.org/TR/WCAG22/#redundant-entry) |

## 9. Tests (for the tester)

- Vitest + Testing Library in jsdom, `fetch` stubbed by URL and method; no bff runs.
- Query by role and accessible name, with names taken from `en_US.json` through `t()` (as
  `fe/src/Instances.test.tsx` does), never by CSS class or test id.
- Failure paths first: each error key, a malformed reply, a network failure.
- Layout, colour and contrast are not tested in jsdom; the designer's review checks them in the code.
