# Brief 13: the host in the navigation, the About page, boot from the list (slice 13)

`PLAN.md` step 10 item 13 (user, 2026-11-19): **an About place lists every version** (behind sign-in:
the framework's version, each installed engine, the GPU backend packages with their versions; what is
installed, not what a running instance loaded) and **the navigation lists hosts, each holding its
instances**. `ARCHITECTURE.md` "Slice 13's decisions" (2026-11-20) gives the bff's part: `GET /api/node`
and `boot` in the items of `GET /api/instances`. Criteria are numbered **13 N.n**; test names cite them
so (`13 N.4`). One slice, 13 criteria.

## 0. What exists (read on main at db9cc8a)

- **`GET /api/node`** (bff slice 13, designed, not yet served; `fe/briefs/api-contract.md`): behind a
  session (401 `session-required` without one), asks the manager nothing, moves no idle deadline. 200:
  `{"name": "<host name>", "api": "/api", "version": "2.99.1", "engines": [{"engine": "llama.cpp",
  "version": "1:1.0.0+pai2", "backends": [{"name": "cuda", "version": "13.5.0+nccl.2.32.3"}, {"name":
  "rocm", "version": "10.1.0"}, {"name": "vulkan", "version": "1.4.363.1"}]}]}`. `name` is never empty
  (`localhost` when the host name is); a `version` is a string or `null` (an empty version file);
  `engines` is `[]` when none is found (a tree build, a package older than slice 13); `backends` is `[]`
  for a CPU-only engine and sorted by name; a backend's `name` is the package suffix (`cuda`, `rocm`,
  `vulkan`), its display name the fe's string. No error but 401.
- **`GET /api/instances`** items gain **`boot`**: `true`, `false`, or `null` when that one
  `BootEnabled` read failed (the list still 200). `GET /api/instances/{id}/boot` stays, for the
  instance page.
- **The nav** (`fe/src/App.tsx` `NavInstances`): Overview; the label "Instances" (`nav-instances`, a
  `p.nav-label`) over a `ul` labelled by it, one link per instance, shown only once a list read
  succeeded with one or more instances; the Profiles group; nothing after it.
- **The Overview** (`fe/src/Overview.tsx`, `Instances.tsx`): `h1` Overview, the version line
  (`GET /api/version`), a `section` with `h2` "Instances" (`instances-title`) and the tiles. Each tile
  reads `GET /api/instances/{id}/boot` once when it mounts (`Tile`, `readBoot`) and shows the Start at
  boot row only once that answered.
- **`pai-config` and `pai`** show no host, no About and no backend version (the GUI shows its own
  version in the window title, `window-title-versioned`): the web UI is first, and nothing is copied.

## 1. Decisions

1. **The host is the group label of its instances** (preference: smallest change, and it scales to N
   by repeating the group). In the nav, the label over the instance links reads the node's `name`
   instead of "Instances"; on the Overview, the `h2` over the tiles does the same. Until
   `GET /api/node` answers, and whenever it fails, both keep today's text ("Instances"), so a failed
   read costs a word, never the list. The host label is text, not a link, and has no icon.
2. **One host is built; N is designed, not built.** Hub mode (step 14) gives one group per node of
   `GET /api/nodes`, in its order, each label with its own id and its list labelled by it; with two or
   more hosts each label becomes a disclosure button (`aria-expanded`) that folds its instance list.
   With one host there is nothing to fold (`fe/DESIGN.md` principle 4, no empty chrome), so this slice
   has no disclosure, no button and no `aria-expanded` in the nav.
3. **The nav and the Overview's heading share one read of `GET /api/node` per session**, sent when a
   session starts (the app opening with one, a sign-in), not at each page change. That shell read
   never signs the app out: a 401 there leaves the label at "Instances", and the page's own reads say
   the session ended (a sign-out from the shell could drop unsaved edits with no question). A sign-in
   in a page's dialog as the same user does not read again; another user does.
4. **About is a nav entry and a page of its own**, `#/about`, the last entry of the nav, after the
   Profiles group, in its own `ul` like Overview, with the `info` icon (already in `Icon.tsx`). It is
   where every version lives (preference over a footer or a block on the Overview: one place to send
   someone asking "what is installed", and in hub mode one section per node). About reads
   `GET /api/node` itself, each time it opens, since an engine upgrade does not restart the bff
   (`ARCHITECTURE.md`: read at each request). The Overview keeps its version line.
5. **About's layout, per host** (Value list, `fe/DESIGN.md` 5.2, class `assets`; no new class):

       h1  About
       h2  <name>                          (data, the host name)
       dl  Private AI Framework | 2.99.1
       h3  llama.cpp                       (data, the engine id as sent)
       dl  Version              | 1:1.0.0+pai2
           CUDA                 | 13.5.0+nccl.2.32.3
           ROCm/HIP             | 10.1.0
           Vulkan               | 1.4.363.1

   One `h3` and one `dl` per engine, in the reply's order; its backends in the reply's order (the
   bff sorts them by name). Versions are data: shown as sent, in monospace (`mono`), never parsed or
   reworded. NCCL is not split out of the CUDA version: the `+nccl.<version>` part of the package
   version already says it, and a Windows CUDA package has none (preference: no parsing of a format
   `xtask` owns).
6. **Display names** of the three known backends are strings (`backend-cuda` "CUDA", `backend-rocm`
   "ROCm/HIP", `backend-vulkan` "Vulkan"); any other `name` is shown as sent (data, so a backend added
   later reads as its package suffix until it gets a string). The engine id is data (`fe/DESIGN.md`
   2.2), never a string.
7. **The empty and unknown cases.** A `version` that is `null`, empty, or not a string reads
   "Unknown" (`about-version-unknown`) in the body font and `--muted` (class `muted`), as the Value
   list's "(none)". `engines: []` gives, after the framework list, one paragraph saying no installed
   engine was found and why a package may not list one. `backends: []` gives the engine's list one
   more row, "GPU backends" with "None, CPU only".
8. **Tiles read `boot` from the list.** The Overview sends no `GET /api/instances/{id}/boot`; the Start
   at boot row shows "On" for `true`, "Off" for `false`, and no row for `null`, a missing field or any
   other value, as today's failed read. The row is there with the tile (no row arriving later), and it
   follows each new list answer. The instance page is unchanged: it keeps its own boot read.
9. **Collapsing an instance** (PLAN: collapsed it shows only its engine, port and tags): the tile is
   that collapsed view and the instance page the expanded one, so no tile gets a disclosure in this
   slice (preference; question Q1). The tile's engine line keeps its rule (shown only when the
   instances span two engines, `fe/DESIGN.md` 2.2; question Q2).

## 2. The texts

| Key | English |
|---|---|
| `nav-about` | `About` |
| `about-title` | `About` |
| `about-loading` | `Loading the versions…` |
| `about-failed` | `The versions could not be read.` |
| `about-version` | `Version` |
| `about-version-unknown` | `Unknown` |
| `about-backends` | `GPU backends` |
| `about-backends-none` | `None, CPU only` |
| `about-no-engine` | `No installed engine was found on this host. An engine package from before this version lists none until it is installed again.` |
| `backend-cuda` | `CUDA` |
| `backend-rocm` | `ROCm/HIP` |
| `backend-vulkan` | `Vulkan` |

Reused, unchanged: `app-name` ("Private AI Framework", the `dt` of the framework row), `page-title`,
`nav-instances` and `instances-title` (the fallback labels), `instances-boot-on`, `instances-boot-off`,
`boot-label`. No other key is added or changed. `…` is U+2026, as in the other loading texts.

## 3. Criteria

**13 N.1 Boot from the list.** `Instance` gains `boot`: `true` or `false` when the item's `boot` is a
boolean, else `null`. On the Overview a tile shows the row `boot-label` / `instances-boot-on` for
`true` and `instances-boot-off` for `false` in the first render of that list answer, and no Start at
boot row for `null`, an absent `boot`, `"true"` or `1`. No tile row means the `dl` rule of today (no
`dl` when no fact has a row).

**13 N.2 No tile boot read.** Opening the Overview, and staying on it through any number of list
answers, sends no request to any `/api/instances/{id}/boot` path. When a later list answer flips an
item's `boot`, the tile's row follows it.

**13 N.3 The shell's node read.** With a session (at start, or after a sign-in on the sign-in page),
the app sends exactly one `GET /api/node`; moving to another page (Overview, an instance, Model
profiles, About, back) sends no further one from the shell. A sign-out then a sign-in as another user
sends one more; a sign-in in a page's dialog as the same user sends none. A 401, any other status, a network failure or a malformed reply (13 N.10) signs
nobody out and leaves both labels at their fallback text.

**13 N.4 The host in the nav.** Once the shell's read answered 200 with `name` `gpu-box`, the nav's
instance group label (`p.nav-label`) reads `gpu-box` and the instances `ul` has the accessible name
`gpu-box` (`aria-labelledby`, as today); its links, their order and `aria-current` are unchanged. Before
the answer and after a failure the label reads `nav-instances` ("Instances"). The rule of today holds:
no label and no list until a list read gave one or more instances. The nav holds no button and no
`aria-expanded` but the Menu button's.

**13 N.5 The host on the Overview.** With the same answer, the Overview's instances `section` has the
`h2` `gpu-box`; before it and after a failure, `instances-title` ("Instances"). The `h1`, the version
line, the tiles and their order are unchanged.

**13 N.6 The About entry.** Signed in, the nav's last item is a link `nav-about` ("About") to
`#/about`, with the `info` icon first (`svg.icon`, `aria-hidden`), in a `ul` of its own after the
Profiles group's list; it has `aria-current="page"` on `#/about` and none elsewhere. Signed out there
is no nav (unchanged).

**13 N.7 The About page.** At `#/about`, signed in: `document.title` is `About | Private AI
Framework`, the `h1` reads `about-title` ("About") and takes the focus on arrival, as every page. While
its read is pending the page shows `about-loading` in a `role="status"` paragraph and no list. Each
arrival at `#/about` sends one `GET /api/node` of the page's own (leaving and coming back sends
another). Signed out at `#/about`, the sign-in page shows with the notice `signin-required` and no
`GET /api/node` is sent (the rule of `fe/briefs/09-signin-only.md` S1a.7; `#/about` is not the
Overview).

**13 N.8 About, the versions.** With the reply of section 0: an `h2` reading `gpu-box`; then a `dl`
with one `dt` `app-name` and its `dd` `2.99.1`; then an `h3` reading `llama.cpp` and a `dl` whose rows
are, in order, `about-version` / `1:1.0.0+pai2`, `backend-cuda` / `13.5.0+nccl.2.32.3`,
`backend-rocm` / `10.1.0`, `backend-vulkan` / `1.4.363.1`. Each version `dd` has the class `mono`;
both `dl`s have the class `assets`. Two engines give two `h3` with their lists, in the reply's order.
A backend named `opencl` shows the `dt` `opencl`. Nothing on the page takes focus but what the shell
holds.

**13 N.9 About, unknown and empty.** A `version` of `null`, `""` or a number (framework, engine or
backend alike) shows `about-version-unknown` ("Unknown") in a `dd` with the class `muted` and not
`mono`. `engines: []` shows the framework `dl`, then one paragraph `about-no-engine`, and no `h3`.
An engine with `backends: []` (or with no `backends` field) shows, after its Version row, the row
`about-backends` / `about-backends-none`, the `dd` with the class `muted`.

**13 N.10 About, failures.** The page's read answering 401 signs the app out as every page read does:
the sign-in page at `#/about` with `signin-session-ended` and the focus on its `h1`
(`fe/briefs/09-signin-only.md` S1b.1). Any other status, a network failure, or a malformed reply
shows `about-failed` as a plain paragraph (not `role="alert"`: nobody acted) in place of the lists,
the `h1` kept. Malformed: not an object, `name` not a non-empty string, or `engines` not an array.
Inside a good reply, an engine item without a non-empty string `engine` is left out, as is a backend
without a non-empty string `name`; the rest of the page shows.

**13 N.11 Order and keyboard.** Tab from the start, signed in at `#/about` at 1280 px: Skip to content,
the home link, the theme switch, Sign out (the Menu button is hidden at that width), Overview, each instance link, Model profiles, About; the page
adds no Tab stop. Below 960 px the menu holds the same links in the same order, and a click on About
closes it (the existing rule of the nav's links).

**13 N.12 Wrapping.** A `name` of 253 characters with no break point and a version of 128 characters
with none cause no sideways scroll at 320 px: the nav label and the `h2`s are blocks that inherit the
body's `overflow-wrap: break-word`, and the About `dd`s are the Value list's (`.assets dd`:
`min-width: 0`, `overflow-wrap: anywhere`). The stylesheet gains no class and no rule (brief 10 R1.8's
class list, as brief 12 amends it, stays as it is).

**13 N.13 Nothing else.** The instance page, its boot read and its list polls, the Model profiles
pages, the sign-in page and the Overview's version line are unchanged; the strings file gains exactly
the twelve keys of section 2 and changes no other.

## 4. Accessibility (WCAG 2.2 AA)

- The host label names its list (`aria-labelledby`), so a screen reader hears "gpu-box, list, 1 item,
  default, link"; the Overview's `h2` and About's `h2` and `h3` give the heading structure h1 > h2 (host)
  > h3 (engine) ([1.3.1 Info and Relationships](https://www.w3.org/TR/WCAG22/#info-and-relationships)).
  The versions are a description list, term and value
  ([H40](https://www.w3.org/WAI/WCAG22/Techniques/html/H40)).
- About has a title and takes the focus on its `h1` on arrival
  ([2.4.2 Page Titled](https://www.w3.org/TR/WCAG22/#page-titled),
  [2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)); its nav link carries
  `aria-current="page"` ([WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-current)) and its
  current look's non-colour cue is the weight, as every entry
  ([1.4.1 Use of Color](https://www.w3.org/TR/WCAG22/#use-of-color)).
- The loading text is a status message; the failure paragraph appears without a user action, so it is
  not an alert ([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages)). The label
  changing from "Instances" to the host name is not announced (a heading or label is not a live region,
  and nobody asked for it).
- "Unknown" is a word, not a dash or a colour; the backends' display names are words
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).
- The same nav, in the same order, on every page, with About always last
  ([3.2.3 Consistent Navigation](https://www.w3.org/TR/WCAG22/#consistent-navigation)).
- Reflow at 320 px and 200 % text, no sideways scroll (13 N.12;
  [1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)).

## 5. Out of scope

- Hub mode: `GET /api/nodes`, several host groups, their disclosure buttons, About per node (decision
  2). The node's `api` field is not read (the fe keeps `/api`).
- A disclosure on the tiles or the instance links (decision 9, Q1), the engine line always shown (Q2).
- Moving or dropping the Overview's version line; the instance page reading `boot` from the list.
- What a running instance loaded (the requirement is what is installed).

## 6. Older tests that pin something that changes

- `HearthTilesBoot.test.tsx` and the tile parts of `SessionEnded.test.tsx` (`TILE_BOOT`, "a tile boot
  state"), `SessionRevoked.test.tsx` and `Boot.test.tsx` 361: the tile's own boot read is gone
  (13 N.1, N.2); a tile can no longer end the session.
- `test/bff.ts`: the fake needs `GET /api/node` and `boot` in its list items. A suggestion, the
  tester's call: answer `GET /api/node` only when a test asks for it (today unknown routes reject),
  so the tests that find the list named "Instances" (`App`, `HearthNav`, `ProfilesNav`, `SignIn`,
  `ModelProfiles`) keep passing on the fallback (13 N.4).
- `HearthNav.test.tsx` 87 lists the nav's labels; About adds no label (its `ul` has none), so it holds.

## 7. Questions for the user

- **Q1.** Should an instance fold in place (a disclosure on the Overview's tile or under the host in
  the nav, collapsed: engine, port, tags; open: more), or is the tile the collapsed view and the
  instance page the open one, as this brief builds?
- **Q2.** Collapsed, PLAN says an instance shows its engine. Today the tile shows the engine only when
  the instances span two engines (one engine: nothing to tell apart). Keep that, or always show it?
