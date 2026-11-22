# Brief 20: the side menu as Nodes and Profiles (slice N1)

`PLAN.md` "N1, the side menu as Nodes and Profiles" (user, 2026-11-22): the manager's web UI and the
director's are one interface, the director showing more and the manager hiding what does not apply.
The menu has two groups even with one host: **Nodes**, one entry per host and under it its instances,
each named by its id with its server profile beside it ("id + profile", the user's pick:
`default · qwen-27b`), then that host's Node info; and **Profiles**. About stays one. This replaces
the one-host rule of `fe/DESIGN.md` 2.3 (no node group, Node info beside About: briefs 13 decisions
1 to 3, 18 decision 3) and brings brief 18 section 8's hub-mode grouping forward, for one host.
Criteria are numbered **20 N.n**; test names cite them so (`20 N.4`). One slice, 12 criteria.

## 0. What exists and what the API answers

- The nav today (`fe/src/App.tsx` 453 to 526): `ul` Overview; the instance group (`p#nav-instances`
  reading the host name or "Instances", `ul` of instance links, shown only once a list read gave
  instances, 13 N.4); `p#nav-profiles` "Profiles" and its `ul` with Model profiles; a last
  unlabelled `ul` with Node info then About.
- The host name: the shell's one `GET /api/version` per session, field `name` (18 AN.15; App.tsx
  165 to 187), `null` while pending and after any failure. Unchanged by this slice.
- `GET /api/instances` already answers `serverProfile` per item: "the name the record of
  `instances.ini` carries, as the manager spells it, `null` when its section names none"
  (`crates/bff/README.md`, "The instances from the manager"). No field is missing. The fe's reader
  drops it today (`fe/src/Instances.tsx` 24 to 33, `Instance` has no such field).
- No server profile page exists in the fe yet (`fe/src/route.ts` has no `#/profiles/server`); it
  comes with slice 17a (brief 16).
- `pai-config` has no nav of nodes or instances (one host, the instance `default`, tabs): the web is
  first here, as for the server profile pool (designer playbook).

## 1. Decisions

1. **Overview stays first, alone, outside both groups** (preference). It is the landing page and the
   home link's target, and in hub mode it is the director's view of every node, so it belongs to no
   one node. Its `h2` over the tiles keeps its rule (host name, or `instances-title`).
2. **With one host the host entry is a plain label, not a disclosure** (preference, with the reason):
   a disclosure folding the only node would add a Tab stop and a click in front of every instance
   and give nothing back, the empty chrome of `fe/DESIGN.md` principle 4; and a label is what
   brief 13 decision 2 already designed for one host. The structure is the hub's own (a list of
   hosts, each holding a labelled list), so hub mode swaps the label for a button with
   `aria-expanded` that controls the host's list, and adds nothing else. "The manager hides what
   does not apply": the fold.
3. **The host entry is always there, so Node info never vanishes.** It reads the host name, or
   "This node" while the shell's read is pending or failed. Brief 18 decision 3 kept Node info out
   of the instance group only because that group appeared late (13 N.4); now the instance links come
   and go inside a group that is always shown.
4. **Routes: keep `#/node` and `#/instances/<id>`** (preference). Moving to `#/nodes/<node>` now
   would make the address depend on a read that can fail (the host name) and on a node id
   `GET /api/nodes` does not give yet. Hub mode adds `#/nodes/<node>` and rewrites `#/node` to the
   local node in place, with no history entry, as 05b rewrote `#/models` (5b.8).
5. **Instance link: id, then the server profile, one inline run.** Visible: `default · qwen-27b`.
   Heard: "default, server profile qwen-27b". The separator and the visible profile are hidden from
   the reader and a visually hidden phrase carries the profile instead, so the name says what the
   second word is ([2.4.4](https://www.w3.org/TR/WCAG22/#link-purpose-in-context)) and still holds
   every visible word in order
   ([2.5.3 Label in Name](https://www.w3.org/TR/WCAG22/#label-in-name); its Understanding page lets
   punctuation such as the `·` be left out). The profile is data (the manager's spelling), never
   translated. With no profile (`null`, missing, empty, not a string) the link is the id alone, as
   today. The profile is shown even when it equals the id (`default · default`): it says which file
   runs (preference; question Q2).
6. **Profiles holds only pages that exist**: Model profiles today. Server profiles joins as its first
   item with slice 17a, which builds its page (principle 4; question Q1).
7. **Icons on the Nodes entries, as on the Profiles entries** (user, 2026-11-22, symmetry). The host
   label gets Lucide `computer` (a machine; distinct from Server profiles' `server`, which is a file
   of settings, and from `hard-drive`, a disk); each instance link one shared `box` (one running unit
   on the host); Node info keeps `cpu`. All decorative (`aria-hidden`), first child, before the text.
   The group labels "Nodes" and "Profiles" stay text only (preference): they name categories, not
   places, and the symmetry the user asked for is entry to entry; an icon on a label would also put
   two icons in a row at the host (`Nodes` then `computer`).
8. **Smallest style** (preference): no token or colour. The host label reuses `.nav-label`; the
   host's list reuses `.nav ul[aria-labelledby]` (indent and 2 px `--line` rule), so the instances sit
   one indent deeper. The visible profile has no colour of its own (no new contrast pair on
   `--accent-soft`). Two rules are needed by the icons: the host label lays its icon out as the
   entries do, and the instance text keeps breaking anywhere (an entry with an icon takes
   `break-word`, `.nav a:has(> .icon)`, which leaves a 40-character id one unbreakable word: the
   min-content trap of `fe/DESIGN.md` 4). Measured in the review, not pinned: an instance pill is
   about 164 px wide at 1280 px and 244 px at 320 px; after the icon and its gap
   `default · qwen-27b` wraps to two lines in the 224 px column and to two or three at 320 px with
   200 % text, the profile on its own line beside the icon, never under it; wrapping is allowed,
   sideways scroll is not (1.4.10).

## 2. The texts

| Key | English |
|---|---|
| `nav-nodes` | `Nodes` |
| `nav-this-node` | `This node` |
| `nav-instance-profile` | `, server profile {0}` |
| `nav-instance-profile-shown` | `· {0}` |

`nav-instances` ("Instances") is removed: the nav was its only user (App.tsx 618). Reused, unchanged:
`nav-label`, `nav-overview`, `nav-profiles`, `nav-model-profiles`, `nav-node`, `nav-about`,
`instances-title`. `·` is U+00B7. The leading comma and space of `nav-instance-profile` are part of
the text (punctuation is language).

## 3. Criteria

**20 N.1 Keys.** `en_US.json` gains the four keys of section 2 with that English, loses
`nav-instances`, and changes no other.

**20 N.2 The reader.** `Instance` gains `serverProfile: string | null`: the item's `serverProfile`
when it is a non-empty string, else `null` (missing, `null`, `""`, a number). Nothing else of the
reader changes; the Overview's tiles do not show it.

**20 N.3 The nav's structure.** Signed in, the `nav` (name `nav-label`) holds, in DOM order:
1. a `ul` with no label holding the Overview link (unchanged);
2. `p#nav-nodes.nav-label` reading `nav-nodes`, then `ul[aria-labelledby="nav-nodes"]` holding
   exactly one `li`; that `li` holds `p#nav-host.nav-label` (the host label, N.4) then
   `ul[aria-labelledby="nav-host"]`, whose items are the instance links (N.5) then the Node info link
   (N.7);
3. `p#nav-profiles.nav-label` and its `ul` with Model profiles (N.8);
4. a last `ul` with no label holding About only (N.9).
No `button` and no `aria-expanded` inside the nav.

**20 N.4 The host label.** `p#nav-host` reads the host name once the shell's read answered with one,
and `nav-this-node` before that and after any failure (the five failures of 13 N.3's
`HOST_FAILURES`), after a first child `svg.icon` of `computer` (`aria-hidden`); its list's accessible
name is the text alone (`gpu-box`, the icon adding nothing). It is text: no link, no button, no
Tab stop. It
moves from the fallback to the name when the read lands, the links unchanged. Sign out and a sign-in
as another user bring the fallback back until that session's read answers (13 N.3, unchanged).

**20 N.5 The instance links.** One link per instance of the latest successful list, in the
manager's order, `href` `instanceHref(id)`, `aria-current="page"` on that instance's page only.
The link holds two children: a first `svg.icon` of `box` (`aria-hidden`), then `span.nav-instance`.
With `serverProfile` `"qwen-27b"` that span holds, in order: the id as text; a
`span.visually-hidden` reading `nav-instance-profile` with `{0}` the profile; a space; a
`span[aria-hidden="true"]` reading `nav-instance-profile-shown` with `{0}` the profile. Its
accessible name is `default, server profile qwen-27b`; its visible text (the hidden span left out) is
`default · qwen-27b`. With `serverProfile` `null` the span holds the id only and the link's name is the id.
A 40-character id with no break point and a 40-character profile wrap inside the pill (no sideways
scroll at 320 px).

**20 N.6 No instances to show.** While the first list read is open, after it failed, and when it
answered `[]`, the host's list holds the Node info link only: the Nodes label, the host label and
Node info are there, no instance link. A later failed read keeps the links of the last success
(today's rule).

**20 N.7 Node info.** The host's list ends with the Node info link: `href="#/node"`, the `cpu` icon
first (`svg.icon`, `aria-hidden`), text `nav-node`, `aria-current="page"` on `#/node` only. The route,
title, `h1` and focus of 18 AN.5 are unchanged.

**20 N.8 Profiles.** `p#nav-profiles` reading `nav-profiles` and `ul[aria-labelledby="nav-profiles"]`
holding the Model profiles link only, with its `aria-current` rules of 2d.64 (`page` on the list,
`true` on a profile's page). No Server profiles link and no `#/profiles/server` route in this slice.

**20 N.9 About.** The last `ul` of the nav has no `aria-labelledby` and holds one item, the About
link (`info` icon, `aria-current="page"` on `#/about`), last in the nav.

**20 N.10 Keyboard and menu.** Tab order at 1280 px with two instances: Skip to content, home link,
theme switch, Sign out, Overview, each instance link in the manager's order, Node info, Model
profiles, About, then the page. Labels take no focus. Below 960 px the Menu button opens the nav
with the same links in the same order; a click on any of them (an instance, Node info, Model
profiles, About) closes it; Escape closes it with the focus on the Menu button (unchanged rules).

**20 N.11 Two rules, no token.** `index.css` gains exactly two rules and no custom property:
`.nav-label:has(> .icon)` with `display: flex`, `align-items: center`, `gap: var(--space-2)` (the
entries' layout, H8a.5), and `.nav-instance` with `min-width: 0` and `overflow-wrap: anywhere`. The
nested host list takes `.nav ul[aria-labelledby]`. `HearthTokens.test.ts` H1a.12 is unchanged (no
size); brief 10 R1.8's class list gains `nav-instance`. `Icon.tsx` gains `computer` and `box`, the
path data of lucide-static 1.74.0 `icons/computer.svg` and `icons/box.svg`
(`https://unpkg.com/lucide-static@1.74.0/icons/<name>.svg`, both answer 200), each with a comment
citing 20 N.4 or N.5.

**20 N.12 Signed out and the Overview.** Signed out, or while the session is read, there is no nav
(S1a.4, unchanged). The Overview's `h2` over the tiles still reads the host name or
`instances-title` (13 N.5, unchanged).

## 4. Accessibility (WCAG 2.2 AA)

- Groups are lists named by visible labels, the hosts a list of their own, so a reader hears the
  hierarchy: "Nodes, list, 1 item; gpu-box; gpu-box, list, 3 items; default, server profile
  qwen-27b, link; ..." ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)).
- The new icons are decorative and hidden, so names are unchanged by them
  ([1.1.1](https://www.w3.org/TR/WCAG22/#non-text-content)).
- Instance names: decision 5 ([2.4.4](https://www.w3.org/TR/WCAG22/#link-purpose-in-context),
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)).
- The current page keeps `aria-current="page"` and its weight cue
  ([WAI-ARIA 1.2, `aria-current`](https://www.w3.org/TR/wai-aria-1.2/#aria-current),
  [1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)); the menu is the same on every page and its
  order does not change with the page ([3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation)).
- Focus order follows the visual order and labels take no stop
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)); targets keep the 32 px pill
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
- Reflow at 320 px and 200 % text with two indents and long ids and profiles
  ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow), [1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)).
- No disclosure with one host, so no `aria-expanded` state to keep true
  ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value) applies in hub mode).

## 5. Older tests that change

The nav's order lists (Node info moves from before About to after the instance links; "Instances"
becomes "Nodes" and the host label): `App.test.tsx` 91 to 119, 273 to 330 (the list named
`nav-instances` is now the one named by the host label); `HearthNav.test.tsx` 91 (the labels become
`nav-nodes`, `nav-this-node` or the host, `nav-profiles`) and 101; `NodeNav.test.tsx` 464 to 560
(13 N.4: fallback `nav-this-node`; "no instances, no label" becomes N.6, the host label and Node info
stay), 488 (no button, still true), 561 (13 N.6: the last `ul` holds About only), 663 and 697 (13 N.11
order); `NodePage.test.tsx` (18 AN.6 order); `ProfilesNav.test.tsx`; `ModelProfiles.test.tsx` 566, 583,
894; `ModelProfilesFilter.test.tsx` 526; `ModelProfileContent.test.tsx` 670; `InstancePage.test.tsx`
1149; `HearthIcons.test.tsx` 283 to 325 (H8a.2's list of nav icons gains `computer` on the host label and
`box` on each instance link; "instance links get none" is reversed; the `cpu` icon now inside the
host list); `Reflow.test.ts` (R1.8's class list gains `nav-instance`). Tests that find
an instance link by the name `default` keep working only while the fixture's item has no
`serverProfile`: `fe/src/test/bff.ts` leaves it out by default (the tester's call, as brief 18
section 6 advised for `name`) and adds it where a test asks for N.5.

## 6. The UX check (`fe/ux/stub.py`, `fe/ux/check.js`)

- `stub.py`: the one instance answers `"serverProfile": "qwen-27b"`; `list=many` gives `default`
  `"qwen-27b"`, `coder` `null`, `embed` missing, and the 40-character `MANY_ID` a 40-character profile
  with no break point.
- `check.js`: a new state `nav-open` (`list=many`, `#/`, click `#menu-button`; expect `#nav-nodes`
  reading `nav-nodes` and `#nav-host`), so the open menu is checked at 320 px, both themes and 200 %
  text (at 1280 px the nav is always shown). The existing states keep their markers.

## 7. Out of scope

- Hub mode: more than one host, the disclosure, `#/nodes/<node>`, the `#/node` rewrite, About for
  the director.
- The Server profiles entry and page (17a), the Overview's tiles showing the server profile, any
  change to the instance page.
- A change to the shell's host read.

## 8. Questions for the user (notes, nothing waits on them)

- **Q1** Server profiles in the menu: (recommended) it appears with its page in slice 17a, the next
  profile slice, since an entry for a missing page is empty chrome; or now, leading to a "coming"
  page.
- **Q2** An instance whose server profile has its own id (`default · default`): (recommended) show it
  anyway, the same form for every instance; or show the profile only when it differs from the id.
