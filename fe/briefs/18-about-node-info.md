# Brief 18: About keeps two facts, the node gets its own page (slice AN5)

`PLAN.md` "About and node info" (user, 2026-11-22): "in About keep only the version of Private AI
Framework and the operating system with its version of where the server (or the director) runs. Move
the other info into the node info and add the version of the engines (llama.cpp, strata etc), the
operating system and its version, the GPU driver versions." The replies are those of `ARCHITECTURE.md`
"About and node info" (accepted 2026-11-22), slices AN1 to AN4, not yet built: the tests use the fake
(`fe/src/test/bff.ts`) and the stub with the shapes of section 0. Criteria are numbered **18 AN.n**;
test names cite them so (`18 AN.4`). Two sub-slices, each under 12 criteria (designer playbook):
**AN5a** (About, the route, the nav, the reader) then **AN5b** (the node page's GPU part).

## 0. What exists and what the API answers

- **Today** (`fe/src/About.tsx`, `node.ts`, read on main at bcc42eb): `#/about` reads `GET /api/node`
  at each arrival and shows `h2` host name, the framework row, one `h3` + `dl.assets` per engine with
  its backends (brief 13 N.7 to N.12). The shell reads `GET /api/node` once per session for the host
  name (the nav group label and the Overview's `h2`, 13 N.3 to N.5). There is **no node page**: the
  GPUs appear only in the model profile page's GPU card (`GET /api/node/devices`, brief 14), which is
  the engine's probe (names and memory) and stays as it is. `pai-config` and `pai` show none of this:
  the web is first, nothing to copy.
- **`GET /api/version`** (AN4, amended by the architect after section 9): `{"version": "2.99.1",
  "name": "gpu-box", "os": {"name": "Ubuntu", "version": "26.04", "kernel": "7.0.0-43-generic"}}`;
  `name` the host name read by the bff itself (never empty, as `/api/node`'s), any of the three `os`
  fields a string or `null`. The machine the web service runs on (the director's in step 14). No
  manager, 401 or 200. **The shell now takes the host name from this read** (AN.15), so the nav label
  no longer waits for the manager; `/api/node` is the Node info page's read only.
- **`GET /api/node`** (AN4): today's `{"name", "api", "version", "engines"}` plus
  `"os": {"name", "version", "kernel"}`, `"vulkan": "ok" | "none" | "failed"` and `"gpus": [{"pci":
  "0000:03:00.0", "vendor": "1002", "name": "AMD Radeon AI PRO R9700", "driver": "amdgpu", "version":
  "7.1.9.31600000", "vulkan": {"driver": "radv", "info": "Mesa 25.2.0"} | null}]`, every text a string
  or `null`, rows in bus order, at most 32. When the manager does not answer within 10 s it is still
  200: `os`, `vulkan` and `gpus` are `null` and `"nodeInfo": {"key": "client-unavailable", "args": []}`
  (any other client failure: `"key": "client-other"`). 403 `session-not-administrator` (the session is
  ended), 401 `session-required`. Facts per field (architecture table): `version` `null` means the
  driver sets no module version (an in-tree amdgpu, i915, xe), so its version is the kernel's; `driver`
  `null` means no driver is bound; a row's `vulkan` `null` with `"vulkan": "ok"` means Vulkan did not
  see that device; `vulkan.driver` and `vulkan.info` both `null` means a device below Vulkan 1.2
  without `VK_KHR_driver_properties`; a Vulkan device that could not be joined by bus id is its own
  row with `pci`, `driver` and `version` `null`.
- **Cost the page must allow for**: `/api/node` now asks the manager, which may spawn its Vulkan
  child (8 s bound, cached 10 minutes): a cold read can take several seconds, so the skeleton
  (brief 17 U17a) will be seen. The fe's 15 s `GET` limit stays above the bff's 10 s.

## 1. Decisions

1. **About shows two rows** (the user's words), no heading under the `h1`: the framework's version
   and the operating system with its version, both from `GET /api/version`. About no longer reads
   `/api/node`. The kernel is not on About (the user named the OS and its version only); it is on the
   node page. The Overview's version line is unchanged.
2. **The node's information is a page of its own, `#/node`, "Node info"** (preference: one place to
   send someone asking "what is on this machine", it reads the manager so it should not slow the
   Overview, and in hub mode it becomes `#/nodes/<node>`, one per node). It reads `GET /api/node` at
   each arrival, as About did (an upgrade does not restart the bff). No polling.
3. **Nav**: "Node info" (`cpu` icon) is the first item of the last unlabelled `ul`, right before
   About (preference: the instance group appears only once a list read gave instances, 13 N.4, so a
   link inside it could vanish). In hub mode it moves into each node's group, under its label
   (designed, not built).
4. **Node page layout** (Value lists, `fe/DESIGN.md` 5.2, no new component, no new class):

       h1  Node info
       dl.assets  Host name            | gpu-box                 (mono)
                  Operating system     | Ubuntu 26.04            (mono)
                  Kernel               | 7.0.0-43-generic        (mono)
                  Private AI Framework | 2.99.1                  (mono)
       h2  Engines
       h3  llama.cpp                   (today's engine block, unchanged: Version, the backends)
       h2  GPUs
       h3  GPU 1: AMD Radeon AI PRO R9700
       dl.assets  Vendor               | AMD
                  PCI address          | 0000:03:00.0            (mono)
                  Driver               | amdgpu 7.1.9.31600000   (mono)
                  Vulkan driver        | radv Mesa 25.2.0        (mono)
       h3  GPU 2: ...

   One `h3` + one `dl` per GPU in the reply's order. The heading is numbered from 1 in that order, so
   two cards of the same model (this machine has two R9700) get distinct headings in a screen
   reader's heading list ([2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels)). Every row is
   always present, so each GPU reads the same way; a missing fact is a word, never a blank.
5. **Joined values, never parsed.** The OS value is `name` and `version` joined by one space, or the
   one that is a non-empty string, or "Unknown" (muted) when neither is. The Driver value is `driver`
   and `version` joined by one space. The Vulkan value is `vulkan.driver` and `vulkan.info` joined by
   one space, or the one present. Values are data, shown as sent (`mono`).
6. **Vendor**: `1002` "AMD", `10de` "NVIDIA", `8086` "Intel" from the strings (the architecture says
   the fe names them, as the backends); any other string is shown as sent, in `mono`; `null` is
   "Unknown".
7. **The manager down (`nodeInfo` present, or `gpus` not an array)**: the page still shows the host,
   the framework and the engines (the bff's own facts). Operating system and Kernel read "Unknown";
   under `h2` GPUs one plain paragraph (no alert: nobody acted) says why, `client-unavailable` with
   its own text, any other key or a missing `nodeInfo` the general one.
8. **Copy versions** (brief 17 U17f, not built) now belongs on the node page, where the versions are:
   when U17f is built its button follows the node page's last list and its copied text follows this
   page's order. About gets no copy button (two short rows, selectable). The coordinator updates U17f's
   fixture lines when it is scheduled.

## 2. The texts

New keys (`…` is U+2026):

| Key | English |
|---|---|
| `nav-node` | `Node info` |
| `node-title` | `Node info` |
| `node-loading` | `Loading the node's information…` |
| `node-failed` | `The node's information could not be read.` |
| `node-host` | `Host name` |
| `system-os` | `Operating system` |
| `system-kernel` | `Kernel` |
| `node-engines` | `Engines` |
| `node-gpus` | `GPUs` |
| `node-gpus-none` | `No GPU was found on this node.` |
| `node-info-unavailable` | `The manager did not answer, so the operating system, the kernel and the GPUs are not shown. Reload the page to try again.` |
| `node-info-failed` | `The manager could not read the operating system, the kernel and the GPUs.` |
| `node-gpu-title` | `GPU {0}: {1}` |
| `node-gpu-title-unnamed` | `GPU {0}` |
| `node-gpu-vendor` | `Vendor` |
| `node-gpu-pci` | `PCI address` |
| `node-gpu-driver` | `Driver` |
| `node-driver-in-kernel` | `(part of the kernel, so its version is the kernel's)` |
| `node-driver-none` | `No driver in use` |
| `node-gpu-vulkan` | `Vulkan driver` |
| `node-vulkan-unreported` | `Not reported by the device` |
| `node-vulkan-not-seen` | `Not seen by Vulkan` |
| `node-vulkan-none` | `No Vulkan driver installed` |
| `node-vulkan-failed` | `Could not be read` |
| `vendor-amd` | `AMD` |
| `vendor-nvidia` | `NVIDIA` |
| `vendor-intel` | `Intel` |

Kept and reused, text unchanged: `nav-about`, `about-title`, `about-loading`, `about-failed`, `app-name`
(the framework row on both pages), `about-version-unknown` ("Unknown", both pages), and, now used by
the node page only, `about-version`, `about-backends`, `about-backends-none`, `about-no-engine`,
`backend-cuda`, `backend-rocm`, `backend-vulkan` (no rename: preference, it would only churn tests).
No key is removed.

## 3. Criteria, AN5a: About, the route, the nav, the reader

**18 AN.1 Keys.** `en_US.json` has the 27 keys of section 2 with that English and changes no other.

**18 AN.2 About reads the version.** At `#/about` the page sends one `GET /api/version` of its own
per arrival (leaving and coming back sends another) and no `GET /api/node`. The shell's one read per
session is now a `GET /api/version` (AN.15), not the page's: the page sends its own beside it.
About does not show `name`. Title, `h1` and focus as 13 N.7. Pending: `about-loading` as the
visually hidden `role="status"`, and after 300 ms a skeleton of **two** `.skeleton` lines (17a.4 now
two, not four).

**18 AN.3 About's rows.** With the reply of section 0 the page holds, after the `h1`, exactly one
`dl.assets` with two rows in order: `app-name` / `2.99.1`, `system-os` / `Ubuntu 26.04`, both `dd` with
class `mono`; no `h2`, no `h3`, no kernel, no host name. OS joins per decision 5: `{"name": "Ubuntu",
"version": null}` shows `Ubuntu`; `{"name": null, "version": "26.04"}` shows `26.04`; both `null`, `os`
`null`, `os` missing (an older bff) or not an object shows `about-version-unknown` in a `dd.muted`
(not `mono`). A `version` that is `null`, `""` or not a string shows the same muted "Unknown".

**18 AN.4 About's failures.** 401 signs out as 13 N.10 (`signin-session-ended`, focus on the sign-in
`h1`); any other status, a network failure, the 15 s limit, or a body that is not an object shows
`about-failed` as a plain paragraph (no `role="alert"`), the `h1` kept.

**18 AN.5 The route.** `parseRoute('#/node')` is `{ page: 'node' }`; `#/node/` and `#/node/x` are
not-found. At `#/node` signed in, `document.title` is `Node info | Private AI Framework`, the `h1`
reads `node-title` and takes the focus on arrival. Signed out at `#/node`: the sign-in page with
`signin-required`, and no `GET /api/node` (S1a.7).

**18 AN.6 The nav.** Signed in, the last `ul` of the nav holds two items in order: a link `nav-node`
to `#/node` with the `cpu` icon first (`svg.icon`, `aria-hidden`; `Icon.tsx` gains `cpu`, the path
data of lucide-static 1.74.0 `icons/cpu.svg`), then the About link (unchanged). `aria-current="page"`
on `#/node` for Node info only. Tab order at 1280 px: Skip to content, home link, theme switch, Sign
out, Overview, each instance link, the Profiles links, Node info, About; the node page adds no Tab
stop. Below 960 px the menu holds the same order and a click on Node info closes it (13 N.11's rule).

**18 AN.7 The reader.** `readNode` keeps today's rules for `name` and `engines` (a reply that fails
them is `node: null`), and adds: `os` (an object of three `string | null`, each `null` unless a
non-empty string; `null` when not an object), `vulkan` (`'ok' | 'none' | 'failed'`, anything else
`null`), `gpus` (`null` unless an array; items not objects are left out; each field `null` unless a
non-empty string; `vulkan` an object of `driver`, `info` or `null`), `nodeInfoKey` (the `nodeInfo`
key string or `null`). A reply with only today's four fields still gives the node, with
`os`, `vulkan`, `gpus` `null` (the node page then shows AN.13's `node-info-failed`). The shell no
longer calls `readNode` (AN.15).

**18 AN.8 The node page's top and engines.** At `#/node`, one `GET /api/node` per arrival. With the
reply of section 0: one `dl.assets` with rows in order `node-host` / `gpu-box`, `system-os` /
`Ubuntu 26.04`, `system-kernel` / `7.0.0-43-generic`, `app-name` / `2.99.1` (all `dd.mono`); then an
`h2` `node-engines` and today's engine blocks (13 N.8, N.9: `h3` per engine, Version and backend
rows, `about-backends-none`, unknown versions, `about-no-engine` for `engines: []`) under it; then an
`h2` `node-gpus`. Unknown OS or kernel reads `about-version-unknown` in `dd.muted`.

**18 AN.9 The node page's states.** Pending: `node-loading` as the visually hidden status and after
300 ms six `.skeleton` lines (17a's rule). 401: signed out with `signin-session-ended`. 403
`session-not-administrator`: signed out with `session-revoked` (the `sessionRevoked` path every read
takes). Any other status, network failure, the 15 s limit or a malformed reply (AN.7): `node-failed`
as a plain paragraph in place of everything under the `h1`.

## 4. Criteria, AN5b: the GPUs

**18 AN.10 One block per GPU.** Under `h2` GPUs, per row in order, an `h3` and a `dl.assets` with
exactly four rows: `node-gpu-vendor`, `node-gpu-pci`, `node-gpu-driver`, `node-gpu-vulkan`. The `h3`
is `node-gpu-title` with the 1-based index and `name`, or `node-gpu-title-unnamed` with the index when
`name` is `null`. Vendor: `1002`, `10de`, `8086` give `vendor-amd`, `vendor-nvidia`, `vendor-intel`
(body font); another string as sent in `mono`; `null` "Unknown" muted. PCI: `mono`, `null` "Unknown"
muted. With the three rows of section 7 (in `stub.py` and `test/bff.ts`'s `NODE` alike) the headings read `GPU 1: AMD Radeon AI PRO R9700`,
`GPU 2: NVIDIA GeForce RTX 5090`, `GPU 3: Intel Arc B580`.

**18 AN.11 The driver row.** `driver` and `version` both set: one `mono` text `amdgpu 7.1.9.31600000`.
`version` `null`: the driver name in a `span.mono`, a space, then `node-driver-in-kernel` in a
`span.muted`. `driver` `null` (whatever `version`): `node-driver-none` in `dd.muted`.

**18 AN.12 The Vulkan row.** By the reply's `vulkan` and the row's: row `vulkan` with both strings
gives `radv Mesa 25.2.0` in `mono`; with one, that one; with both `null`, `node-vulkan-unreported`
muted. Row `vulkan` `null`: reply `"ok"` gives `node-vulkan-not-seen`, `"none"` gives
`node-vulkan-none`, `"failed"` or any other value gives `node-vulkan-failed`, each muted.

**18 AN.13 No GPU, or no answer.** `gpus: []` gives one paragraph `node-gpus-none` under the `h2` and
no `h3` there. `gpus` `null` (or not an array): one paragraph, `node-info-unavailable` when
`nodeInfoKey` is `client-unavailable`, otherwise `node-info-failed`; no `h3` under GPUs; the top list
reads "Unknown" for OS and kernel; host, framework and engines still show. Neither paragraph has a
`role`.

**18 AN.14 Wrapping.** At 320 px, base and 200 % text: a 256-byte unbroken GPU `name`, `driver`,
`vulkan.info` and a 253-character host name cause no sideways scroll (`h3` and `h2` are blocks with the
body's `overflow-wrap: break-word`; the `dd`s are `.assets dd`, `overflow-wrap: anywhere`, and the
driver row's spans are inline inside it, so `anywhere` applies to them). 32 rows render 32 blocks.
The stylesheet gains no rule: `.assets + h3` already spaces each GPU heading; `h2` after a list takes
its default margin (to check in the UX shots; a rule only if it reads as a row, as in brief 13).

**18 AN.15 The shell reads the host from `/api/version`** (AN5a; added after section 9, numbered
last to keep the others stable). With a session, the shell's one read per session (13 N.3: at start,
after a sign-in on the sign-in page, again after a sign-out and sign-in, none for a sign-in in a
page's dialog as the same user, none on a page change) is a `GET /api/version`, and the shell sends
no `GET /api/node` at all. Its `name`, when a non-empty string with something besides spaces, is the
nav group label and the Overview's `h2` (13 N.4, N.5); before the answer, on 401 (signing nobody
out), any other status, a network failure, or a `name` missing, empty or not a string, both keep
"Instances". The Overview's version line keeps its own read and is unchanged. Only the Node info
page sends `GET /api/node`.

That is 10 criteria in AN5a (AN.1 to AN.9 and AN.15) and 5 in AN5b (AN.10 to AN.14).

## 5. Accessibility (WCAG 2.2 AA)

- Heading structure h1 page > h2 Engines, GPUs > h3 engine, GPU; values in description lists
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  [H40](https://www.w3.org/WAI/WCAG22/Techniques/html/H40)); numbered GPU headings stay distinct for
  identical cards ([2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels)).
- Both pages have titles and take focus on their `h1`
  ([2.4.2](https://www.w3.org/TR/WCAG22/#page-titled), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
  Node info's link carries `aria-current="page"`
  ([WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-current)) and the same place on every page
  ([3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation)).
- Loading is a status message; the failure and manager-down paragraphs are not alerts, as nobody acted
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)). A screen reader hears, for example,
  "GPU 1: AMD Radeon AI PRO R9700, heading level 3", then "Driver, amdgpu 7.1.9.31600000".
- Missing facts are words ("Unknown", "Not seen by Vulkan"), never a dash or a colour; the muted
  colour passes 4.5:1 as in every Value list ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color),
  [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)).
- Reflow at 320 px and 200 % text ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow),
  [1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)); the pages add no control, so no new target.

## 6. Older tests that change

- `AboutPage.test.tsx` (13 N.7 to N.12): N.7's "one GET /api/node of its own" becomes `/api/version`
  (AN.2); N.8, N.9's engine and backend cases move to the node page (AN.8); N.10's malformed rules
  for About are AN.4's; N.12's class and stylesheet checks hold (no new class or rule).
- `U17aSkeletons.test.tsx` 321 to 391: About's skeleton is two lines and waits on `version`, not
  `node`; the node page is added to 17a.4's list with `node-loading` and six lines.
- Nav order lists that end in About now hold Node info before it: `App.test.tsx` 96, 118, 312;
  `ModelProfiles.test.tsx` 566, 583, 894; `ProfilesNav.test.tsx`; `HearthNav.test.tsx` 44 (no new
  label); `NodeNav.test.tsx` 497 (13 N.6: the last `ul` now holds Node info then About, About still
  last), 565 and 598 (13 N.11 tab and menu order); `HearthIcons.test.tsx` 283 (H8a.2 gains the `cpu`
  icon on Node info).
- `NodeNav.test.tsx` 162 to 400 (13 N.3) and every test that counts the shell's `GET /api/node`:
  the shell's one read per session is now `GET /api/version` (AN.15), and walking through `#/node`
  sends no further shell read. 13 N.4 and N.5 tests take the name from the `version` answer.
- `test/bff.ts`: `NODE` gains the `os`, `vulkan` and three `gpus` of section 7; the `version` default
  becomes `{"version": "2.99.0", "name": "gpu-box", "os": {...}}`. Careful: a default `name` would
  turn every test that finds the list named "Instances" (13 N.4's fallback) red; suggestion, the
  tester's call: leave `name` out of the default and add it only where a test asks for the host.
- `Version.test.tsx` and the Overview's version line: unchanged (the extra fields are ignored).
- `HearthTokens.test.ts` and brief 10 R1.8's class list: unchanged (no class, no size).

## 7. The UX check (`fe/ux/stub.py`, `fe/ux/check.js`)

- `stub.py`: `/api/version` answers `{"version": "3.0.0", "name": "gpu-box", "os": {"name": "Ubuntu", "version":
  "26.04", "kernel": "7.0.0-43-generic"}}`. `NODE` gains that `os`, `"vulkan": "ok"` and three GPUs:
  `0000:03:00.0` `1002` "AMD Radeon AI PRO R9700" `amdgpu` `7.1.9.31600000` `{radv, Mesa 25.2.0}`;
  `0000:0a:00.0` `10de` "NVIDIA GeForce RTX 5090" `nvidia` `580.95.05` `{NVIDIA, 580.95.05}`;
  `0000:0f:00.0` `8086` "Intel Arc B580" `xe` `null` and `vulkan` `null` (in-kernel driver, not
  seen). A state `node=down` answers `os`, `vulkan`, `gpus` `null` and `nodeInfo`
  `client-unavailable`; `node=long` answers 32 GPUs, each `name`, `driver`, `vulkan.info` 256
  unbroken characters, `name` of the node 253.
- `check.js`: `about` expects `dl.assets` count 1 (was 2). New states: `node` (`#/node`, `h1`
  `node-title`, `dl.assets` count 5: top, one engine, three GPUs), `node-down` (`node=down`,
  `dl.assets` count 2, the `node-info-unavailable` paragraph), `node-long` (`node=long`, no sideways
  scroll is already checked for every state), and the node page in the loading state (`load=slow`).

## 8. Out of scope

- Hub mode: `#/nodes/<node>`, Node info inside each node's nav group, About for the director.
- Copy versions (U17f, decision 8), the devices probe (VRAM, `GET /api/node/devices`), live
  monitoring (the later Hardware page), and polling the node page.
- Any change to the shell's read beyond AN.15, the Overview, the instance and profile pages.

## 9. Note for the coordinator (settled)

The shell's once-per-session `GET /api/node` would have asked the manager and waited up to 10 s
cold. Settled by the architect (2026-11-22): `GET /api/version` carries `name`, and the shell reads
it there (AN.15).

## AN5 review (designer, 2026-11-22): accepted

Read against the code and tests, and checked in Chromium, Firefox and WebKit with `fe/ux/stub.py`
(`build/ux-review/an5.js`, shots in `build/ux-review/an5/`): About, Node info with `node=ok`, `down`
and `long`, and three replies routed in Playwright for the muted words (driver `null`, in-kernel,
unknown vendor id, PCI `null`, Vulkan `none`, `failed`, unreported, driver only, `gpus: []`); 1280
and 320 px, 100 % and 200 % text, light and dark: 168 states, sideways scroll 0 in every one.

- AN.1 met: `fe/src/strings/en_US.json` 468 to 494, the 27 keys and their English.
- AN.2, AN.3 met: `fe/src/About.tsx` 23 to 59, one `GET /api/version` per arrival, a two-line
  skeleton, one `dl.assets` of two rows; the OS joins by `Value.tsx` `joined`. Seen: "3.0.0",
  "Ubuntu 26.04", no host, no kernel.
- AN.4 met, with one refinement: a 403 `session-not-administrator` signs out as revoked
  (`About.tsx` 29, `AboutPage.test.tsx` 320), as every read does under 09 S1b.1; the brief's "any
  other status" is read as excluding it.
- AN.5 met: `route.ts` 18; title "Node info | Private AI Framework"; after a click the focus is on
  the `h1` in all three engines.
- AN.6 met: `App.tsx` 503 to 525; `cpu` path data of lucide-static 1.74.0 (`Icon.tsx` 95 to 112);
  Tab order seen in all three engines: Skip to content, home, theme, Sign out, Overview, default,
  Model profiles, Node info, About; at 320 px a click on Node info closes the menu.
- AN.7 met: `node.ts` 65 to 203.
- AN.8, AN.9 met: `NodePage.tsx` 238 to 270; with `/api/node` held 6 s the page shows the hidden
  status `node-loading` and six skeleton lines.
- AN.10 to AN.13 met: `NodePage.tsx` 81 to 211; every muted word seen in the right row, the
  vendor names in the body font, an unknown id in mono; `node=down` keeps host, framework and
  engines and shows the `node-info-unavailable` paragraph with no role.
- AN.14 met: 32 blocks with 256-character values and a 253-character host wrap at 320 px and 200 %;
  the spacing of `h2` after a list and of `.assets + h3` reads as sections, so no rule is needed.
- AN.15 met: `App.tsx` 180; with `/api/node` held, the nav group label reads "gpu-box" at once.
- Screen reader view (Playwright aria snapshot): h1 Node info, the terms and definitions of the
  host list, h2 Engines, h3 llama.cpp, h2 GPUs, h3 "GPU 1: AMD Radeon AI PRO R9700" ..., in reading
  order; the in-kernel driver reads as one definition "xe (part of the kernel, ...)".
