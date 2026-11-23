// Brief 10, slice R1 (fe/briefs/10-reflow.md), the part jsdom can pin: the
// stylesheet. Every criterion of R1 is a declaration or a markup shape, so the
// file reads index.css as text, the way HearthH2b.test.ts and S1a.test.ts do
// (comments dropped, the same rule regex, the same nesting depth, prop() the
// last value of a property in a block, norm() a value with its runs of
// whitespace collapsed); those files export no helper and importing a test
// file would register its tests here, so the reader is a small local copy.
//
// What jsdom cannot see is left to the designer's measurement (brief 10
// sections 4 and 5, fe/ux): the scrollWidth of every page at 200 % text in
// 320 px, the table's box scrolling inside itself, the focus ring whole at the
// box's edge when Tab scrolls it, the letters inside the notice's border, the
// file name broken inside the instance intro's column, the sign-in pair
// centred at 1280 and 1600 px, and the box's edge drawn in forced colours.
// The DOM half of R1.3 is in ModelProfiles.test.tsx.
//
// R1.8's strings half is pinned by the suite's own strings lints, not here:
// `strings.test.ts` "every key the code uses has a value, and every value is
// used" scans src for t('key') calls, so a key this slice adds with no code to
// read it is red there; `App.test.tsx` "en_US.json holds the English the brief
// gives" pins the text of the keys its EN table names, so no string is changed
// or removed. A sha of the whole file pinned the same thing and reds every
// slice that legitimately adds a string, so it is gone.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  flat: string
  body: string
  at: number
  depth: number
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  // Whitespace dropped: the selector is written over several lines.
  compact: m[1].replace(/\s+/g, ''),
  // Runs of whitespace become one space, so a needle can name a descendant
  // combinator ("main > *"), which compact cannot.
  flat: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the order of the rules inside an @media block says
// nothing about the cascade of the top-level ones.
const TOP = RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

// Every property a block declares, in the order it declares them.
const declared = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .filter((name) => name !== '' && !name.startsWith('--'))

// A value, as written, with the whitespace a formatter may have put in
// collapsed to one space.
const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length
          ? ` (${list.map((r) => r.sel.replace(/\s+/g, ' ')).join(' | ')})`
          : ''),
    )
  return list[0]
}

// The top-level rule whose selector is exactly this.
const topBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === sel.replace(/\s+/g, '')),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
  )

// The offset of the brace that closes the block opened at `open`.
const rangeEnd = (open: number): number => {
  const start = CSS.indexOf('{', open)
  if (start < 0) throw new Error(`no block opens at ${open}`)
  let depth = 0
  for (let i = start; i < CSS.length; i++) {
    if (CSS[i] === '{') depth++
    if (CSS[i] === '}' && --depth === 0) return i
  }
  throw new Error(`the block at ${open} is never closed`)
}

// [open, end] of every block whose header is this at-rule, in file order.
const atRanges = (header: string): Array<[number, number]> => {
  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(header, from)
    if (at < 0) return out
    const end = rangeEnd(at)
    out.push([at, end])
    from = end + 1
  }
}

// A rule as "selector = value": how a property that several rules set is
// pinned as one list, so a rule that drops its declaration or changes its
// value is named, and a new rule that sets it is named.
const sets = (name: string): string[] =>
  RULES.filter((r) => prop(r.body, name) !== '')
    .map((r) => `${r.flat} = ${norm(prop(r.body, name))}`)
    .sort()

// The declarations of one rule, checked as the brief writes them: each value
// as written, and the property list exactly (a property the brief does not
// list is red).
function declares(
  rule: Rule,
  what: string,
  list: Array<[string, string]>,
  message: string,
): void {
  for (const [name, value] of list)
    expect(norm(prop(rule.body, name)), `${what}: ${name}`).toBe(value)
  expect(
    declared(rule.body),
    `${what}: exactly ${list.map(([n]) => n).join(', ')}: ${message}`,
  ).toEqual(list.map(([n]) => n))
}

// ── R1.1: the inherited break ─────────────────────────────────────────────

test('10 R1.1: the body rule declares overflow-wrap: break-word, and every rule that sets overflow-wrap itself keeps the value it has today', () => {
  const body = topBy('body', 'the body rule')
  expect(
    norm(prop(body.body, 'overflow-wrap')),
    'the inherited break that splits a word with no break point, the instance intro\'s "(server@default.ini)." of 0.1 and a card title (CSS Text 3, overflow-wrap: break-word breaks only where a line has no other break point)',
  ).toBe('break-word')
  // The whole sheet: the rules that set overflow-wrap, with their value. The
  // twelve of H1a, H5a, H6a, H8a, H9a and 2d keep theirs (an element's own
  // declaration beats the inherited one, whatever the specificity), R1.2 adds
  // the notice and brief 12 section 3 adds the tile's facts. A rule beside
  // these that sets it is a break no brief asked for.
  expect(
    sets('overflow-wrap'),
    'the rules that set overflow-wrap, each with its value: R1.1 adds body, R1.2 adds .notice, 12 section 3 adds .tile-facts dd, 15 section 10 R3 adds the chip span inside its 15em block, R5 adds the expanded pill and 15 section 11 D2 adds the option line, 20 N.11 adds .nav-instance, 16 17a review D1 adds the three cells of the Server profiles table, the rest are unchanged',
  ).toEqual(
    [
      'body = break-word',
      '.notice = anywhere',
      'h1 = anywhere',
      '.session > span = anywhere',
      '.nav a = anywhere',
      '.nav a:has(> .icon) = break-word',
      'th, td = anywhere',
      '.model-profiles th = normal',
      '.model-profiles td:nth-child(2) = normal',
      // Brief 16 slice 17a, its 17a review (D1, 2026-11-23): the same break the
      // Model profiles columns keep, for the Server profiles table.
      '.server-profiles th = normal',
      '.server-profiles td:nth-child(2), .server-profiles td:nth-child(4) = normal',
      '.server-profiles td:nth-child(3) = normal',
      '.tile-id = anywhere',
      '.tile-facts dd = anywhere',
      '.model-profile-summary = anywhere',
      '.jump ul button = anywhere',
      '.full-value = anywhere',
      '.assets dd = anywhere',
      // Brief 14, DESIGN.md 5.2 "GPU list" (GD1.12): the row's label breaks a
      // 64-character id inside the `li`, at 320 px and at 200 % text.
      '.check-list > li > .check = anywhere',
      // Brief 15 section 10 R3 (the tags as chips): the chip's text is cut
      // with an ellipsis, and only where the user's own text size makes the
      // viewport under 15 text widths does it break: that rule, in that block,
      // is the one that sets it. The chip itself sets nothing (R3: no
      // overflow-wrap on .chip).
      '.chip > span = anywhere',
      // Its section 10 R5: the pill a cut chip opens is several lines wide, so
      // the tag breaks inside it. Its R7: the rule is `.chip > .chip-full`, the
      // pill being a span child of the chip (a bare `.chip-full` loses to the
      // cut of `.chip > span` and shows the ellipsis itself).
      '.chip > .chip-full = anywhere',
      // Brief 15 section 11, the C2 review (D2): a 32-character suggestion at
      // 320 px and 200 % text scrolls the page sideways, the option line's
      // unbroken label is the flex item's min-content; the break splits it.
      '.tag-options li = anywhere',
      // Brief 20 20 N.11: the text run of an instance link breaks a 40-character
      // id and server profile with no break point, inside the pill, at 320 px.
      '.nav-instance = anywhere',
    ].sort(),
  )
  for (const r of RULES.filter((r) => prop(r.body, 'overflow-wrap') !== ''))
    expect(
      (r.body.match(/overflow-wrap\s*:/g) ?? []).length,
      `${r.flat}: overflow-wrap declared once, so no later declaration undoes it`,
    ).toBe(1)
})

// ── R1.2: the notice ──────────────────────────────────────────────────────

test('10 R1.2: the .notice rule declares overflow-wrap: anywhere, its box is unchanged, and the icon rule is untouched', () => {
  declares(
    topBy('.notice', 'the notice rule'),
    'the notice',
    [
      ['padding', 'var(--space-2) var(--space-3)'],
      ['border', '1px solid var(--control)'],
      ['border-radius', 'var(--radius)'],
      ['background', 'var(--surface)'],
      ['display', 'flex'],
      ['align-items', 'flex-start'],
      ['gap', 'var(--space-2)'],
      ['overflow-wrap', 'anywhere'],
    ],
    'the seven of H8b.5 plus the break of R1.2, and no min-width or flex declaration: break-word leaves a flex item at its longest word, anywhere is what lowers it (CSS Text 3, overflow-wrap)',
  )
  declares(
    topBy('.notice > .icon', 'the notice icon rule'),
    'the notice icon',
    [['margin-top', 'calc((1lh - 1.25em) / 2)']],
    'the icon is an svg with no text: R1.2 leaves the rule alone (0,2,0, so it wins over the notice for the icon)',
  )
})

// ── R1.4: the scroll box ──────────────────────────────────────────────────

test('10 R1.4: .table-scroll is one new top-level rule: overflow-x auto, the scroll padding, --bg, the card radius, the shadow and the transparent 1px border, and nothing else', () => {
  const box = topBy('.table-scroll', 'the scroll box of R1.3')
  declares(
    box,
    'the scroll box',
    [
      ['overflow-x', 'auto'],
      ['scroll-padding-inline', 'var(--space-2)'],
      ['background', 'var(--bg)'],
      ['border-radius', 'var(--radius-card)'],
      ['box-shadow', 'var(--shadow)'],
      ['border', '1px solid transparent'],
    ],
    'no overflow-y, no height, no width: the box is as tall as the table and takes the width the page gives it (R1.4)',
  )
  expect(
    box.depth,
    'written at the top level, not inside a media block: the box scrolls at every width',
  ).toBe(0)
})

// ── R1.5: the table loses the lift ────────────────────────────────────────

test('10 R1.5: table keeps border-collapse, border-spacing, --bg and 100%, and no longer declares a radius, a shadow or a border; the cells are unchanged', () => {
  const table = topBy('table', 'the table rule')
  for (const [name, value] of [
    ['border-collapse', 'separate'],
    ['border-spacing', '0'],
    ['background', 'var(--bg)'],
    ['width', '100%'],
  ] as Array<[string, string]>)
    expect(norm(prop(table.body, name)), `kept on the table: ${name}`).toBe(
      value,
    )
  for (const name of ['border-radius', 'box-shadow', 'border'])
    expect(
      norm(prop(table.body, name)),
      `${name}: the lift moved to the box, which clips what its child paints outside it (R1.4, R1.5)`,
    ).toBe('')
  // The cells stay as H2b.3 left them: the headers on one line, so the table
  // is 286 px at 320 px and 100 % text and nothing scrolls (measured, 0.1).
  declares(
    topBy('.model-profiles th', 'the Model profiles headers'),
    'th',
    [
      ['white-space', 'nowrap'],
      ['overflow-wrap', 'normal'],
    ],
    'the header line stays one line, as H2b.3 left it',
  )
  declares(
    topBy('.model-profiles td:nth-child(2)', 'the Engine column'),
    'td:nth-child(2)',
    [
      ['white-space', 'normal'],
      ['overflow-wrap', 'normal'],
    ],
    'the engine column wraps as H2b.3 left it',
  )
  declares(
    topBy('.model-profiles td:last-child', 'the Status column'),
    'td:last-child',
    [['white-space', 'nowrap']],
    'the switch cell stays one line, as H2b.3 left it',
  )
  expect(
    prop(
      topBy('tr:last-child td', "the last row's divider").body,
      'border-bottom',
    ),
    'the last row keeps dropping its divider over the box radius',
  ).toBe('0')
})

// ── R1.6: forced colours ──────────────────────────────────────────────────

test('10 R1.6: no forced-colours block is added, none of them names the scroll box, and the boxes drawn with the shadow token are the card, the dialog, the bar, the panel, the tile and the box, with no table', () => {
  expect(
    atRanges('@media (forced-colors: active)').length,
    'the five blocks of H1a, H2a, H6a.10 and H8b stay five (R1.6): the box needs its transparent border, not a block, where a shadow computes to none (CSS Color Adjustment 1)',
  ).toBe(5)
  const named = atRanges('@media (forced-colors: active)')
    .flatMap(([open, end]) =>
      RULES.filter((r) => r.depth > 0 && r.at > open && r.at < end),
    )
    .filter((r) => r.compact.includes('.table-scroll'))
    .map((r) => r.flat)
  expect(
    named,
    'no forced-colours rule names the scroll box: its 1px transparent border is the edge there',
  ).toEqual([])
  const lifted = RULES.filter(
    (r) => prop(r.body, 'box-shadow') === 'var(--shadow)',
  )
    .map((r) => r.flat)
    .sort()
  expect(
    lifted,
    'the shadow token draws exactly these boxes: table is gone, .table-scroll took the lift (R1.4, R1.6)',
  ).toEqual([
    // The brief's six, in the sorted order of the list above.
    '.card',
    '.dialog',
    '.save-bar',
    '.signin-panel',
    '.table-scroll',
    '.tile',
  ])
})

// ── R1.7: the sign-in pair is centred ─────────────────────────────────────

test('10 R1.7: .signin-page gains margin-inline: auto after its four declarations, and main > * is still a max-width with no margin of its own', () => {
  declares(
    topBy('.signin-page', 'the sign-in page rule'),
    'the sign-in page',
    [
      ['display', 'grid'],
      [
        'grid-template-columns',
        'repeat(auto-fit, minmax(min(100%, 18rem), 1fr))',
      ],
      ['gap', 'var(--space-6)'],
      ['align-items', 'center'],
      ['margin-inline', 'auto'],
    ],
    'the four of H7.6 in their order, then the centring R1.7 asks for: no other margin, no width (the pair is capped by main > * at 880px, so the margin is 0 below 960px and the narrow layouts do not move)',
  )
  declares(
    topBy('main > *', 'the page width cap'),
    'main > *',
    [['max-width', '880px']],
    'the cap of H7.6 declares the width only: a margin here would compete with the centring (R1.7: 0,1,0 beats 0,0,1)',
  )
})

// ── R1.8: nothing else ────────────────────────────────────────────────────

// Every class the stylesheet selects, sorted, in the sheet's own words. R1.8
// froze the list at one new class, table-scroll; brief 12 section 3 (the T1
// review, 2026-11-20) amended that freeze with exactly six more: port,
// tags-profile, tags-save, tags-status, tags-alert, tags-sign-in; brief 14
// (the GPU distribution card) with three: check-list-actions (the row of Auto
// and Even), gpu-id (the row's monospace id) and gpu-weight (the weight field
// of GD2.1); brief 15 section 3, as its section 10 R1 amends it, with six:
// chips-row, chips, chip, chips-profile, tag-add and tag-new (the chip's x is
// styled as `.chip button`, it has no class of its own), and drops tags-save,
// the button the chips replaced; its section 10 R5 adds the seventh, chip-full,
// the pill a cut chip opens. Brief 15 slice C2 (the tags the node already uses,
// suggested under the pill) adds tag-options. Brief 17, slice U17a (the
// skeleton while a page's
// first read is pending), adds the eighth and ninth: skeleton (a line of the
// shape to come) and skeleton-box (the same, a box tall). Brief 19 slice LS1
// (the block slider, 19 G.12: at least 24 px, accent-coloured, native track)
// adds the tenth: blocks-slider. Its LS1 review (D1, 2026-11-22) adds the
// eleventh: gpu-blocks-line, the shared class of the budget, Auto and
// malformed line, so the 720 px grid can place them beside the GPU list. Brief
// 20 slice N1 (the nodes menu, 20 N.11) adds the twelfth: nav-instance, the
// text run of an instance link, the span that lets the pill shrink and the long
// id wrap. Brief 16 slice 17a's 17a review (the designer, 2026-11-23, D1) adds
// the thirteenth: server-profiles, the class the review amends 17a.11 to give
// the Server profiles table, so its headers, port and Used by cells stay one
// line and its box scrolls at 320 px. Brief 21's TP1 review (2026-11-23, D1)
// adds the row items of the Tensor placement card; the review names their
// declarations and not their names ("named by the class the coder gives them",
// 21 section 6), so the card's classes are allowed by the card's own namespace,
// the `ot` of its strings keys (model-profile-ot-*), and the `tensor`/`placement`
// of its card title: the test names what that namespace is, and every class
// outside these and the frozen list is a look no brief asked for.
const CLASSES =
  `about alert app-name art-bar art-bar-on art-cable art-door art-ground art-knob art-leaf art-line art-panel art-stem art-sun assets assets-card badge banner banner-tools blocks-slider boot-alert boot-status card check check-list check-list-actions chip chip-full chips chips-profile chips-row collapsed dialog dialog-actions engine field field-control field-error field-hint field-label form-error full-value gpu-blocks-line gpu-id gpu-weight icon instance-header intro jump jump-label jump-toggle layout mark menu-button model-profile-back model-profile-summary model-profiles model-profiles-filter model-profiles-filter-hint model-profiles-filter-row model-profiles-filter-status model-profiles-note model-profiles-switch model-profiles-switch-alert model-profiles-switch-status mono muted nav nav-instance nav-label note notice number off open port primary run-actions run-alert save-actions save-alert save-bar save-messages save-status saved saved-restart secondary server-profiles session shape shape-changing shape-failed shape-running shape-stopped signin signin-actions signin-art signin-page signin-panel skeleton skeleton-box skip-link stacked subgroup tag-add tag-new tag-options tags-alert tags-profile tags-sign-in tags-status theme-switch theme-switch-label tile tile-engine tile-facts tile-head tile-id tiles unit visually-hidden weight-group`.split(
    /\s+/,
  )

test('10 R1.8 (as brief 15 section 10 R1 and R5, brief 15 slice C2, brief 17 U17a, brief 19 LS1 and its LS1 review, brief 20 N.11, brief 16 17a review D1 and brief 21 TP1 review D1 amend its section 3 list): the classes the sheet gains are table-scroll, the six of brief 12, the three of brief 14, the six of brief 15 with its R5 pill and its C2 list, the two of brief 17 U17a, the block slider of brief 19 LS1 and its gpu-blocks-line, the nav-instance of brief 20, the server-profiles of brief 16 17a review D1, the Tensor placement classes of brief 21 in their own namespace, and the ids a selector names are the mount point #root, the GPU fieldset the LS1 review places in the grid and the Tensor placement card its D1 places there', () => {
  const used = new Set<string>()
  const ids = new Set<string>()
  for (const r of RULES)
    for (const part of r.sel.split(',')) {
      for (const m of part.matchAll(/\.([a-zA-Z][\w-]*)/g)) used.add(m[1]!)
      for (const m of part.matchAll(/#[\w-]+/g)) ids.add(m[0])
    }
  // Brief 21, its TP1 review (D1): the coder names the classes of the card's
  // row items, so they are kept in the card's namespace (the comment above
  // the frozen list), and the review's 720 px rule selects the card by an id.
  const ofCard = (c: string) => /^(ot|tensor|placement)[-a-z0-9]*$/.test(c)
  const ofTensorCard = (id: string) => /tensor|\bot\b/.test(id)
  expect(
    [...used].filter((c) => !ofCard(c)).sort(),
    'the classes of the sheet, with table-scroll, the six of brief 12 section 3, the three of brief 14, the six of brief 15 section 3 with the pill its section 10 R5 adds (its R1: the chip x has no class), the skeleton of brief 17 U17a, the blocks-slider of brief 19 LS1, the nav-instance of brief 20, the server-profiles of brief 16 17a review D1, tags-save gone, and nothing else outside the Tensor placement namespace (R1.8: a class the designer did not brief is a look nothing asked for)',
  ).toEqual([...CLASSES, 'table-scroll'].sort())
  expect(
    [...used].filter(ofCard).sort(),
    '21 TP1 review D1: the card of brief 21 is styled by at least one class of its own namespace: the row items the review wraps each label and its control in',
  ).not.toEqual([])
  const cardIds = [...ids].filter(ofTensorCard).sort()
  expect(
    [...ids].filter((id) => !ofTensorCard(id)).sort(),
    'the ids a rule selects by: #root, the mount point, and #model-profile-gpu-devices, the GPU fieldset the LS1 review (D1) places in the 720 px grid: no other (R1.8)',
  ).toEqual(['#model-profile-gpu-devices', '#root'])
  expect(
    cardIds.length,
    '21 TP1 review D1: one id of the Tensor placement card, the one its 720 px rule places in the control column, as the LS1 review D1 places #model-profile-gpu-devices: no more (R1.8)',
  ).toBe(1)
  expect(
    cardIds[0],
    '21 TP1 review D1: the id names the Tensor placement card: its heading id model-profile-card-tensor (21 T.2), or the Rules fieldset id the review writes #…ot',
  ).toMatch(/^#model-profile-(card-tensor|ot(-rules)?)$/)
})

// ── 12 T1 layout: the tile's facts break (brief 12 section 3, T1 review) ───

test('12 T1 layout (brief 12 section 3): .tile-facts dd declares overflow-wrap: anywhere, keeping its margin and weight', () => {
  // The facts of a tile are flex items, and a flex item keeps its longest word
  // as its minimum width under the inherited break-word, so a 32-character tag
  // scrolled the Overview sideways at 320 px. The same rule as R1.2's notice,
  // for the same reason (CSS Text 3, overflow-wrap).
  declares(
    topBy('.tile-facts dd', "the value of a tile's fact"),
    'the tile fact value',
    [
      ['margin', '0'],
      ['font-weight', '600'],
      ['overflow-wrap', 'anywhere'],
    ],
    'the two of H6b.6 plus the break brief 12 section 3 asks for: no min-width and no flex property, the break is what lowers the item (.tile-id already carries min-width: 0, this is the value cell)',
  )
})

// ── 15 section 11 (the C2 review): the suggestions box, D1 and D2 ─────────

test('15 C2 review D1: .tag-options declares flex-basis: 100% and margin: 0, so the list takes a full-width row of its own under the chip row', () => {
  // The list is an item of the wrapping flex .field-control (index.css:1358)
  // with no basis, so it shrinks to its widest label (55 to 127 px) and at
  // 1280 px sits to the right of the chip row at the fieldset's top, pushing
  // the row down: section 3's "full width of the field" and decision 10's
  // "right under the chip row" are not met. The fix is the declaration
  // `.field-control .field-error` already carries (index.css:2149): the basis
  // puts it on its own line, and the margin is 0 because the flex row gap
  // already gives --space-2.
  const box = topBy('.tag-options', 'the suggestions list of C2')
  for (const [name, value] of [
    ['flex-basis', '100%'],
    ['margin', '0'],
  ] as Array<[string, string]>)
    expect(
      norm(prop(box.body, name)),
      `D1 (brief 15 section 11): the list is a wrapping-flex item with no basis, so it shrinks to its widest label and sits beside the chip row: ${name}`,
    ).toBe(value)
})

test('15 C2 review D2: .tag-options declares min-width: 0 and .tag-options li declares overflow-wrap: anywhere, so a 32-character suggestion at 320 px and 200 % text breaks inside the page', () => {
  // A 32-character suggestion at 320 px and 200 % text scrolls the page
  // sideways 267 px in all three engines, with or without D1's fix (section
  // 11): a flex item's automatic minimum size is its min-content, and the
  // unbroken label is that min-content. min-width: 0 lets the item go below
  // it (CSS Flexbox 1) and the break in the line is what splits the label
  // (CSS Text 3, overflow-wrap: anywhere): the same pair as the check-list
  // row of brief 14 and the tile fact above.
  const box = topBy('.tag-options', 'the suggestions list of C2')
  expect(
    norm(prop(box.body, 'min-width')),
    'D2 (brief 15 section 11): the list is a flex item floored at its min-content by the auto minimum size, so a long label pushes the page sideways: min-width',
  ).toBe('0')
  expect(
    norm(
      prop(
        topBy('.tag-options li', 'the suggestion line').body,
        'overflow-wrap',
      ),
    ),
    'D2 (brief 15 section 11): the label has no break point inside the line, so the line is as wide as the label: overflow-wrap',
  ).toBe('anywhere')
})

// ── 19 LS1 review (the designer, 2026-11-22): D1, the GPU fieldset's grid ───

test('19 LS1 review D1: in the 720 px media rule that makes .field a 200px + 1fr grid, #model-profile-gpu-devices > .gpu-blocks-line and #model-profile-gpu-devices > .check-list are grid-column: 2, so the budget, Auto and malformed line and the GPU list sit in the control column, and Auto and Even flow onto the row after the list', () => {
  // The GPU list's fieldset is a `.field`, so from 720 px its children place
  // themselves in the two-column grid (index.css 1388): the budget, Auto or
  // malformed paragraph of 19 G.4 and 19 G.5 takes the second cell of the
  // first row, the list drops into the 200 px label column, and Auto and Even
  // stretch to the list's whole height
  // (build/ux-review/ls1/firefox-1280-dark.png). The review's fix: the three
  // paragraphs share one class, and this block puts that class and the list in
  // column 2, so row 1 is the as-of line beside the budget, the list goes
  // under the budget, and Auto and Even land on the row after it. The markup
  // half (the class on each of the three paragraphs) is ModelProfileGpu8.
  const blocks = atRanges('@media (min-width: 720px)')
  expect(blocks.length, 'the sheet has 720 px media blocks').toBeGreaterThan(0)
  const inside = (open: number, end: number) =>
    RULES.filter((r) => r.depth > 0 && r.at > open && r.at < end)
  // The block that styles `.field`: the two rules of D1 must sit in the same
  // block, so they apply wherever the grid does.
  const grids = blocks
    .map(([open, end]) => ({
      open,
      end,
      field: inside(open, end).filter((r) => r.compact === '.field'),
    }))
    .filter((b) => b.field.length > 0)
  expect(
    grids.length,
    'exactly one 720 px block styles .field, the two-column grid of index.css 1389: a second block would place the rules where the grid is not',
  ).toBe(1)
  const grid = grids[0]!
  expect(
    norm(
      prop(
        single(grid.field, 'the 720 px .field rule').body,
        'grid-template-columns',
      ),
    ),
    'the grid D1 is about: a 200px label column and a control column',
  ).toBe('200px minmax(0, 1fr)')
  // A rule of that block, by one of its selectors: the two may be written as
  // one comma group or as two rules, the sheet's own style for a shared
  // declaration (index.css 1399).
  const inGrid = (sel: string): Rule =>
    single(
      RULES.filter(
        (r) =>
          r.depth > 0 &&
          r.at > grid.open &&
          r.at < grid.end &&
          r.sel
            .split(',')
            .some((p) => p.replace(/\s+/g, '') === sel.replace(/\s+/g, '')),
      ),
      `the 720 px rule placing "${sel}"`,
    )
  for (const sel of [
    '#model-profile-gpu-devices > .gpu-blocks-line',
    '#model-profile-gpu-devices > .check-list',
  ]) {
    const rule = inGrid(sel)
    expect(
      norm(prop(rule.body, 'grid-column')),
      `D1: ${sel}: the column beside the 200px label, where the field's control is`,
    ).toBe('2')
    // The three paragraphs come and go with the state (blocks, Auto,
    // malformed), so the rows are placed by the flow, never by hand.
    expect(
      norm(prop(rule.body, 'grid-row')),
      `D1: ${sel}: no grid-row: the lines above the list appear and go, so a hand-placed row breaks with them`,
    ).toBe('')
  }
})

// ── 20 N1 review (the designer, 2026-11-22): D1, the instance text under its icon ──

test('20 N1 review D1: .nav-instance declares flex: 1 1 4rem, so "default · qwen-27b" stays beside its box icon at 1280 px and 100 % text (20 N.11, decision 8)', () => {
  // An instance pill is a wrapping flex row (`.nav a:has(> .icon)`, index.css
  // 565), and the run of words is one of its items. A flex item with no basis
  // is sized to its content, so the run (90 px measured at 1280 px and 100 %)
  // is wider than the room left beside the icon in the 224 px column and moves
  // whole to a line under it, where decision 8 says it never goes at 100 %
  // text. A grow of 1 and a 4rem basis give it the room beside the icon and it
  // wraps inside itself (`overflow-wrap: anywhere`, pinned by R1.1 above, and
  // `min-width: 0`, pinned by H1a.12, which let it shrink at all); at 200 %
  // text the basis no longer fits beside the icon and the run moves whole
  // under it, as every entry does (H8a.5). Measured in the three engines with
  // the rule injected (the review), so this pins the declaration only.
  const run = topBy(
    '.nav-instance',
    'the run of words of an instance link (20 N.5, 20 N.11)',
  )
  expect(
    norm(prop(run.body, 'flex')),
    'D1 (brief 20, N1 review): the run is an item of the wrapping pill with no flex basis, so it takes its content width and drops under the icon at 1280 px and 100 % text: flex',
  ).toBe('1 1 4rem')
})

// ── 21 TP1 review (the designer, 2026-11-23): D1, the Tensor placement rows ──

test("21 TP1 review D1 (the sheet): in the 720 px block that styles .field, one rule puts the Tensor placement card's wrapper div in grid-column: 2, so from 720 px the format line, the rules and Add rule sit in the control column beside the Rules legend, and not in its 200 px label column", () => {
  // `index.css` gained no rule for the card, so at 720 px and wider its
  // wrapper div (21 T.2: everything after the Rules legend is in one div) is
  // the second grid cell of a field with an empty label cell: it lands in the
  // 200 px column, every rule's `.field` grid is `200px 0px`, its control
  // column is 0 px wide, the selects are 18 px and "Remove" stands one letter
  // per line (build/ux-review/tp1/card-1280.png). The review's fix is the LS1
  // D1 one: this block places the div in the control column.
  const grids = atRanges('@media (min-width: 720px)').filter(([open, end]) =>
    RULES.some(
      (r) => r.depth > 0 && r.at > open && r.at < end && r.compact === '.field',
    ),
  )
  expect(
    grids.length,
    "exactly one 720 px block styles .field, the two-column grid (the 19 LS1 review D1 test pins it): the card's rule must sit where the grid is",
  ).toBe(1)
  const [open, end] = grids[0]!
  const placed = RULES.filter(
    (r) =>
      r.depth > 0 &&
      r.at > open &&
      r.at < end &&
      prop(r.body, 'grid-column') !== '' &&
      /\bdiv\b/.test(r.flat),
  )
  const rule = single(
    placed,
    'the 720 px rule placing the card wrapper div (the review D1 writes `#…ot fieldset > div`, as LS1 D1 wrote `#model-profile-gpu-devices > .check-list`)',
  )
  expect(
    norm(prop(rule.body, 'grid-column')),
    "D1: grid-column: the control column, the one beside the 200px label where the field's control is",
  ).toBe('2')
  expect(
    norm(prop(rule.body, 'grid-row')),
    'D1: no grid-row: the empty line, the list and (in TP2) the coverage lines come and go, so the cells are placed by the flow, never by hand',
  ).toBe('')
  expect(
    rule.flat,
    'D1: the selector names the Tensor placement card by an id (its heading id model-profile-card-tensor of 21 T.2, or the Rules fieldset id the review writes #…ot) and a div (21 T.2: the one wrapper div)',
  ).toMatch(/#model-profile-[\w-]*(card-tensor|ot)\b.*\bdiv\b/)
})

test('21 TP1 review D1 (as its D4 re-check of 2026-11-23 amends it): the Tensor item .ot-tensor declares flex: 1 1 16rem, the Goes on item .ot-goes-on flex: 1 1 12rem and the Pattern item .ot-pattern flex-basis: 100%, each with min-width: 0, so at 320 px the items wrap inside the li and never between a label and its control', () => {
  // 21 T.4: the rule's controls are a wrapping flex row and each visible label
  // and its control is ONE item, Tensor asking 16rem, Goes on 12rem, Pattern a
  // line of its own. With no rule in the sheet the items take their content
  // width and the pairs break apart: at 320 px the "Pattern" label ends beside
  // Remove and its input lands on the next line
  // (build/ux-review/tp1/card-320.png). min-width: 0 on each item is what lets
  // it go below its min-content (CSS Flexbox 1), the same pair the 15 section
  // 11 C2 review D2 pins for the suggestions list.
  // The D4 re-check names the item classes and gives the two select items
  // their row declarations (display, flex-wrap, align-items, gap, pinned by
  // TensorPlacement1), so the three sizes are pinned on the named class, not
  // on "the size properties and nothing else". The pair alone is pinned to
  // the card namespace, the new `.ot-… select` rules of D4 included: an item
  // asking a size outside the card is a look no brief asked for (R1.8).
  const sameSet = (got: string[], want: string[]) =>
    got.length === want.length &&
    got.sort().every((n, i) => n === want.sort()[i])
  const declaring = (names: string[]) =>
    TOP.filter((r) => sameSet(declared(r.body), names))
  for (const [sel, name, value, what] of [
    [
      '.ot-tensor',
      'flex',
      '1 1 16rem',
      'the Tensor item of 21 T.4 (a select of 17 options)',
    ],
    [
      '.ot-goes-on',
      'flex',
      '1 1 12rem',
      'the Goes on item of 21 T.4 (a select of devices)',
    ],
    [
      '.ot-pattern',
      'flex-basis',
      '100%',
      'the Pattern item of 21 T.4, on a line of its own',
    ],
  ] as Array<[string, string, string, string]>) {
    const item = topBy(sel, `the ${what} (the class the D4 re-check names)`)
    expect(norm(prop(item.body, name)), `D1: ${what}: ${name}`).toBe(value)
    expect(
      norm(prop(item.body, 'min-width')),
      `D1: ${what} is a flex item floored at its min-content by the auto minimum size, so a long device name pushes the page sideways at 320 px and 200 % text: min-width`,
    ).toBe('0')
  }
  for (const r of declaring(['flex', 'min-width']))
    for (const p of r.flat.split(','))
      expect(
        p.trim(),
        'a rule declaring flex and min-width and nothing else asks a size: the two select items of 21 T.4, and after D4 the two selects it puts inside them, all of the Tensor placement card: every selector of such a rule names the card (R1.8)',
      ).toMatch(/^\.ot-/)
  const basisItems = declaring(['flex-basis', 'min-width'])
  expect(
    basisItems.length,
    'exactly one top-level rule declares flex-basis and min-width and nothing else: the Pattern item, on a line of its own (21 T.4, the review D1)',
  ).toBe(1)
  const pattern = single(basisItems, 'the Pattern item')
  expect(
    norm(prop(pattern.body, 'flex-basis')),
    'D1: the Pattern item takes a full row of the rule, so its label is never beside the Remove button (21 T.4, 21 T.9): flex-basis',
  ).toBe('100%')
  expect(
    norm(prop(pattern.body, 'min-width')),
    'D1: its 120-character pattern has no break point, so the item is as wide as the pattern without this: min-width',
  ).toBe('0')
})

// ── 16 17a review (the designer, 2026-11-23): D1, the Server profiles table ──

test('16 17a.11 as its 17a review D1 amends it: the sheet styles .server-profiles so its headers and its Port and Used by cells stay one line, its Tags cell breaks at the ", " spaces and its id keeps the base break, so the box scrolls at 320 px instead of squeezing', () => {
  // The base rule `th, td { overflow-wrap: anywhere }` (index.css 728) lets every
  // column shrink to one character, so at 320 px the headers "Server profile" and
  // "Used by", the port 8080 and a 32-character id stood one letter per line and
  // the box never scrolled (the review, sp17a/webkit-320-dark-list.png). The fix
  // is the one the Model profiles table beside it already carries (index.css
  // 759, 785): the table takes the class `server-profiles` (the markup half,
  // ServerProfiles1) and these rules unbreak what is one short word, leaving the
  // id to the inherited break and the tags to their ", " spaces. Measured by
  // injection in the three engines at 320 px (the review: 590 of 286 px at
  // 100 %, 1097 at 200 %, the id column 112 px), so this pins the declarations.
  declares(
    topBy('.server-profiles th', 'the Server profiles headers'),
    'the Server profiles headers (16 17a review D1)',
    [
      ['white-space', 'nowrap'],
      ['overflow-wrap', 'normal'],
    ],
    'the two of .model-profiles th (H2b.3): "Server profile", "Port", "Tags" and "Used by" stay one line each',
  )
  declares(
    topBy(
      '.server-profiles td:nth-child(2), .server-profiles td:nth-child(4)',
      'the Port and Used by cells',
    ),
    'the Port and Used by cells (16 17a review D1)',
    [
      ['white-space', 'nowrap'],
      ['overflow-wrap', 'normal'],
    ],
    "a port and an instance id are one word: broken they stand one letter per line, which is what squeezed the table. The review writes the two cells as one comma group, the sheet's style for a shared declaration; the id cell (nth-child(1)) and the tags cell (nth-child(3)) are not in it",
  )
  declares(
    topBy('.server-profiles td:nth-child(3)', 'the Tags cell'),
    'the Tags cell (16 17a review D1)',
    [['overflow-wrap', 'normal']],
    'the break at the ", " spaces is what wraps the tags between tags (17a.3: "text wraps and hides nothing"); no white-space, so the cell keeps wrapping and is never one long line, and no `anywhere`, so a 32-character tag is not split letter by letter',
  )
  for (const r of RULES)
    for (const part of r.flat.split(','))
      expect(
        part.trim(),
        '16 17a review D1: "the id keeps `anywhere`": no rule takes the base break away from the Server profiles id cell',
      ).not.toBe('.server-profiles td:nth-child(1)')
})
