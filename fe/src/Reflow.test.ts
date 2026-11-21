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
    'the rules that set overflow-wrap, each with its value: R1.1 adds body, R1.2 adds .notice, 12 section 3 adds .tile-facts dd, 15 section 10 R3 adds the chip span inside its 15em block and R5 adds the expanded pill, the rest are unchanged',
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
// the pill a cut chip opens. A class beside these is a look no brief asked for.
const CLASSES =
  `about alert app-name art-bar art-bar-on art-cable art-door art-ground art-knob art-leaf art-line art-panel art-stem art-sun assets assets-card badge banner banner-tools boot-alert boot-status card check check-list check-list-actions chip chip-full chips chips-profile chips-row collapsed dialog dialog-actions engine field field-control field-error field-hint field-label form-error full-value gpu-id gpu-weight icon instance-header intro jump jump-label jump-toggle layout mark menu-button model-profile-back model-profile-summary model-profiles model-profiles-filter model-profiles-filter-hint model-profiles-filter-row model-profiles-filter-status model-profiles-note model-profiles-switch model-profiles-switch-alert model-profiles-switch-status mono muted nav nav-label note notice number off open port primary run-actions run-alert save-actions save-alert save-bar save-messages save-status saved saved-restart secondary session shape shape-changing shape-failed shape-running shape-stopped signin signin-actions signin-art signin-page signin-panel skip-link stacked subgroup tag-add tag-new tags-alert tags-profile tags-sign-in tags-status theme-switch theme-switch-label tile tile-engine tile-facts tile-head tile-id tiles unit visually-hidden weight-group`.split(
    /\s+/,
  )

test('10 R1.8 (as brief 15 section 10 R1 and R5 amend its section 3 list): the classes the sheet gains are table-scroll, the six of brief 12, the three of brief 14 and the six of brief 15 with its R5 pill, and the only id a selector names is the mount point #root', () => {
  const used = new Set<string>()
  const ids = new Set<string>()
  for (const r of RULES)
    for (const part of r.sel.split(',')) {
      for (const m of part.matchAll(/\.([a-zA-Z][\w-]*)/g)) used.add(m[1]!)
      for (const m of part.matchAll(/#[\w-]+/g)) ids.add(m[0])
    }
  expect(
    [...used].sort(),
    'the classes of the sheet, with table-scroll, the six of brief 12 section 3, the three of brief 14 and the six of brief 15 section 3 with the pill its section 10 R5 adds (its R1: the chip x has no class), tags-save gone, and nothing else (R1.8: a class the designer did not brief is a look nothing asked for)',
  ).toEqual([...CLASSES, 'table-scroll'].sort())
  expect(
    [...ids].sort(),
    'no rule selects an element by an id, beside the #root the app mounts on (R1.8)',
  ).toEqual(['#root'])
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
