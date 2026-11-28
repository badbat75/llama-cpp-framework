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
    'the rules that set overflow-wrap, each with its value: R1.1 adds body, R1.2 adds .notice, 12 section 3 adds .tile-facts dd, 15 section 10 R3 adds the chip span inside its 15em block, R5 adds the expanded pill and 15 section 11 D2 adds the option line, 20 N.11 adds .nav-instance, 16 17a review D1 adds the three cells of the Server profiles table, 16 17f.8 adds .instance-id, 16 17f2 review D2 adds the display name form’s hint, 21 TP3 adds the coverage line of a rule, the rest are unchanged',
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
      // Brief 16 slice 17f (17f.8): the id line under a name that is not the id
      // is a grid item of the tile and a block of the page header, so a
      // 32-character id with no break point needs the break.
      '.instance-id = anywhere',
      // Brief 16, its 17f2 review (D2, 2026-11-25): the display name form's
      // hint holds the 40-character id with no break point, and its width sets
      // the form's grid column, so the page scrolls sideways at 320 px and at
      // 200 % text (measured 38 to 42 px, and 376 to 379 px).
      '.instance-name-form .field-hint = anywhere',
      // Brief 21, sub-slice TP3 (21 T.15a): the coverage line of a rule counts
      // up to 20 000 tensors in a row that is a few controls wide, so the count
      // breaks anywhere at 320 px and at 200 % text.
      '.ot-rule-row .ot-coverage = anywhere',
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

test('10 R1.4 (as brief 22 22D4 D6 adds its position, and brief 22 L1 re-check D2 its container type): .table-scroll is one new top-level rule: overflow-x auto, the scroll padding, --bg, the card radius, the shadow, the transparent 1px border, position: relative and container-type: inline-size, and nothing else', () => {
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
      // Brief 22 22D4 D6: the box is the positioned ancestor the rows'
      // `.visually-hidden` names are placed against and clipped by (1.4.10).
      ['position', 'relative'],
      // Brief 22 L1 re-check D2: the box is the inline-size container the
      // confirm group's `100cqi` cap measures against, so the question of a
      // confirm row wraps to the width the user sees and not to the width of
      // the table it spans (ListColumns.test.tsx pins the pair).
      ['container-type', 'inline-size'],
    ],
    'no overflow-y, no height, no width: the box is as tall as the table and takes the width the page gives it (R1.4); the position is the anchor of 22 D6 and the container type the L1 re-check D2 adds, both declared after the look, the sheet’s way for a box that only gains a rule',
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
  // 22 D4.23 moves the row's − out of the Status cell and into the Actions
  // column, so `td:last-child` is no longer the Status column. The one-line
  // shape of the Status column and of the new Actions column is pinned off the
  // cells themselves in ListColumns.test.tsx (22 D4.24). The header line and
  // the Engine column, which the amendment leaves where they are, stay here.
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
    'the shadow token draws exactly these boxes: table is gone, .table-scroll took the lift (R1.4, R1.6), and the floating tip joins them (22 D4.19)',
  ).toEqual([
    // The brief's six plus the tip of 22 D4.19, in the sorted order of the list above.
    '.card',
    '.dialog',
    '.hint-tip',
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
// outside these and the frozen list is a look no brief asked for. Brief 16
// slice 17f (17f.8) adds the fourteenth: instance-id, the quiet id line beside a
// name that is not the id, on the page header, a tile and a Used by link. Brief
// 22 slice 22D2 (the status row under the summary line, 22 D2.1) added the
// fifteenth: page-status, the row of the status paragraph and the delete. Brief
// 22 slice 22D4 (22 D4.9) takes it out again, the row leaving both profile
// pages, and adds the sixteenth, seventeenth and eighteenth: confirm-row (22
// D4.4, the row of the confirm a list row's − opens), hint-tip (22 D4.3, the
// reason a disabled − shows on hover and focus) and status-cell (22 D4.1, the
// cell of a list row that wraps its − under the value). Brief 22 slice IP5
// (IP5.1) takes the Status column, the row switch’s status region, the off
// note and the off row out of the Models list, so their classes leave the
// sheet; status-cell and model-profiles-switch stay, because the instance
// page’s Served here cell (17e.4a) carries both until brief 26 (IP6) removes
// that section.
// (2026-11-24, D1 and D2) adds the nineteenth and twentieth: hint-box (the box
// of a − and its tip, the anchor the tip is placed by) and inactive (the − of a
// row that cannot act, drawn as the aria-disabled buttons are). Brief 16 slice
// 17c (17c.6a, the 17c check) adds the twenty-first: run-remove, the Remove
// instance button at the end of the instance page's run row, drawn with
// --danger and --space-5 before it.
const CLASSES =
  `about alert app-name art-bar art-bar-on art-cable art-door art-ground art-knob art-leaf art-line art-panel art-stem art-sun assets assets-card badge banner banner-tools blocks-slider boot-alert boot-status card check check-list check-list-actions chip chip-full chips chips-profile chips-row collapsed confirm-group confirm-row danger dialog dialog-actions engine field field-control field-error field-hint field-label form-error full-value gpu-blocks-line gpu-id gpu-weight hint-box hint-tip icon inactive instance-header instance-id intro jump jump-label jump-toggle layout mark menu-button model-profile-back model-profile-summary model-profiles model-profiles-filter model-profiles-filter-hint model-profiles-filter-row model-profiles-filter-status model-profiles-switch mono muted nav nav-instance nav-label note notice number open port primary run-actions run-alert run-remove save-actions save-alert save-bar save-messages save-status saved saved-restart secondary server-profiles session shape shape-changing shape-failed shape-running shape-stopped signin signin-actions signin-art signin-page signin-panel skeleton skeleton-box skip-link stacked status-cell subgroup tag-add tag-new tag-options tags-alert tags-profile tags-sign-in tags-status theme-switch theme-switch-label tile tile-engine tile-facts tile-head tile-id tiles unit visually-hidden weight-group`.split(
    /\s+/,
  )

test('10 R1.8 (as brief 15 section 10 R1 and R5, brief 15 slice C2, brief 17 U17a, brief 19 LS1 and its LS1 review, brief 20 N.11, brief 16 17a review D1, brief 16 17f.8, brief 21 TP1 review D1, brief 22 22D4 and brief 16 17d review D1 and brief 22 CD amend its section 3 list): the classes the sheet gains are table-scroll, the six of brief 12, the three of brief 14, the six of brief 15 with its R5 pill and its C2 list, the two of brief 17 U17a, the block slider of brief 19 LS1 and its gpu-blocks-line, the nav-instance of brief 20, the server-profiles of brief 16 17a review D1, the instance-id of brief 16 17f.8, the Tensor placement classes of brief 21 in their own namespace, and the ids a selector names are the mount point #root, the GPU fieldset the LS1 review places in the grid and the Tensor placement card its D1 places there, and the confirm-row, hint-tip and status-cell of brief 22 22D4 (22 D4.4, 22 D4.3, 22 D4.1) with the hint-box and inactive its 22D4 review adds (D1, D2), page-status gone with the status row (22 D4.9), the create form’s field wrapper of brief 22 22C1 review D1, the quants field of brief 22 22C2 review A1, and the danger of brief 22 CD, and brief 22 IP5.1 takes the off note’s class and the off row’s out of the sheet, and the status cell and the row switch out of the Models list, their two classes staying for the instance page’s Served here cell until brief 26 removes it', () => {
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
  // Brief 16, its 17f review (D3, 2026-11-23) and slice 17f2 (2026-11-25): the
  // form that sets the display name gains a class the brief describes and does
  // not name, so it is kept in the namespace of the slice's strings keys
  // (`instance-name-*`, `instance-label-cancel`).
  const ofRename = (c: string) =>
    /^instance-(rename|label|name)[-a-z0-9]*$/.test(c)
  // Brief 16, slice 17e (its 17e check of 2026-11-25): the section's status
  // row, its alert and its table are styled by classes the brief describes and
  // does not name, so they are kept in the namespace of the slice's strings
  // keys (`instance-models-*`), as 17f review D3 keeps the rename form.
  const ofModels = (c: string) => /^instance-models[-a-z0-9]*$/.test(c)
  // Brief 16, slice 17d's review (the designer, 2026-11-26, D1): the offer's
  // button row shares the run row's rules, so the offer's classes are kept in
  // the namespace of the slice's strings keys (`restart-offer-*`).
  const ofOffer = (c: string) => /^restart-offer[-a-z0-9]*$/.test(c)
  // Brief 22, slice 22C1's review (the designer, 2026-11-26, D1): the Model
  // file field wraps what the box adds under it (the list, the folder's line,
  // the retry) in one element the 720 px rule places in the control column. The
  // brief describes that element and does not name its class, so it is kept in
  // the namespace of the form's strings keys (`model-files-*`,
  // `model-profiles-new-*`, `model-source-*`, `create-*`) and the field
  // family's `field-after`, as 17f review D3 keeps the rename form.
  const ofCreate = (c: string) =>
    /^(field-after|model-profiles-new|model-files?|model-source|create)[-a-z0-9]*$/.test(
      c,
    )
  // Brief 22, its 22C2 review (the designer, 2026-11-26, A1): the quants field
  // is laid one label per line by classes the review describes and does not
  // name, so they are kept in the namespace of the field's strings keys
  // (`hf-show-quants`, `hf-quants-loading`, `hf-quant-legend`,
  // `hf-quant-default`), as 17f review D3 keeps the rename form.
  const ofQuants = (c: string) => /^hf[-a-z0-9]*$/.test(c)
  // Brief 22 slice L2, its design review (2026-11-27): the code review names
  // the Save bar's Delete by a class of its own (`save-bar-delete`, so a verb
  // added to the bar does not inherit the end-of-row margin and the danger
  // colour), and advice 2 moves a card's About into the row of its h3, which is
  // laid by the sheet. (The round's advice 1, a short verb of the narrow bar in
  // spans of its own, is withdrawn by the round's second review, D4: its two
  // classes leave the sheet with the spans.) The coder names those classes, so
  // they are kept in this slice's namespace, as 17f review D3 keeps the rename
  // form.
  const ofL2 = (c: string) =>
    /^(save-bar-delete|card-heading|card-about)[-a-z0-9]*$/.test(c)
  const ofTensorCard = (id: string) => /tensor|\bot\b/.test(id)
  expect(
    [...used]
      .filter(
        (c) =>
          !ofCard(c) &&
          !ofRename(c) &&
          !ofModels(c) &&
          !ofOffer(c) &&
          !ofCreate(c) &&
          !ofQuants(c) &&
          !ofL2(c),
      )
      .sort(),
    'the classes of the sheet, with table-scroll, the six of brief 12 section 3, the three of brief 14, the six of brief 15 section 3 with the pill its section 10 R5 adds (its R1: the chip x has no class), the skeleton of brief 17 U17a, the blocks-slider of brief 19 LS1, the nav-instance of brief 20, the server-profiles of brief 16 17a review D1, the instance-id of brief 16 17f.8, the confirm-row and hint-tip of brief 22 22D4 with the hint-box and inactive of its 22D4 review, page-status gone with the status row of 22D2 (22 D4.9), tags-save gone, and nothing else outside the Tensor placement namespace, the rename form of brief 16 17f review D3, the restart offer of brief 16 17d review D1 and the quants field of brief 22 22C2 review A1 (R1.8: a class the designer did not brief is a look nothing asked for)',
  ).toEqual([...CLASSES, 'table-scroll'].sort())
  expect(
    [...used].filter(ofRename).sort(),
    '16 17f review D3: the rename form is styled by at least one class of the slice namespace, the one the markup carries: today the form has no class and the sheet lays out none of it',
  ).not.toEqual([])
  expect(
    [...used].filter(ofCard).sort(),
    '21 TP1 review D1: the card of brief 21 is styled by at least one class of its own namespace: the row items the review wraps each label and its control in',
  ).not.toEqual([])
  expect(
    [...used].filter(ofL2).sort(),
    '22 L2 review: the Save bar’s Delete and the card’s heading row are styled by at least one class of this slice’s namespace: the class the code review gives the Delete, and the row advice 2 places its About in (a look keyed on a class, not on a place in the group)',
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

// ── 16 17f.8: the id line breaks (brief 16 slice 17f) ──────────────────────

test('16 17f.8: the .instance-id rule is a quiet small line that breaks anywhere, declaring font-size var(--fs-sm), color var(--muted), overflow-wrap: anywhere and margin 0, and nothing else', () => {
  // The id beside a name that is not the id (17f.3, 17f.4): a 32-character id
  // with no break point, at 320 px and 200 % text, must not scroll the page
  // sideways. The tile's link already breaks (`.tile-id`, pinned by R1.1
  // above), and so does the nav's name (`.nav-instance`, 20 N.11).
  declares(
    topBy('.instance-id', 'the id line rule'),
    'the id line',
    [
      ['font-size', 'var(--fs-sm)'],
      ['color', 'var(--muted)'],
      ['overflow-wrap', 'anywhere'],
      ['margin', '0'],
    ],
    'the four of 17f.8: small and muted as a quiet line, the break for a long id, and no margin so the line sits where the page puts it (--muted on --bg and --surface is an existing token pair, WCAG 1.4.3)',
  )
})

// ── 16 17f review (designer, 2026-11-23): D1 the id span, D3 the form ──────

test('16 17f review D1 (as brief 22 22 D4.34 amends it): no rule scoped to .instance-id makes the id a block: the id of a Used by link is plain text on the name’s line, outside the link', () => {
  // 17f review D1 stacked the id under the name, because it sat INSIDE the
  // link and ran into it ("Coding assistantID default"). 22 D4.34 moves the id
  // out of the link, so the inline span needs no line of its own: the name and
  // the id are one line, the id muted. The markup is pinned in
  // InstanceName17f.test.tsx and ListColumns.test.tsx; the shared `.instance-id`
  // rule keeps the four of 17f.8 (pinned above), which is what makes it quiet.
  const scoped = TOP.filter((r) => r.compact.includes('.instance-id'))
  expect(
    scoped
      .filter((r) => norm(prop(r.body, 'display')) === 'block')
      .map((r) => r.flat),
    'no rule scoped to .instance-id declares display: block: the id of a Used by link is inline text after the link (22 D4.34)',
  ).toEqual([])
})

test('16 17f2.3 (17f review D3): the display name form is laid out by one class of the slice namespace: it starts --space-3 under the title row and stacks, its label is a block above the box, and its buttons are a row with --space-2 between them', () => {
  // 17f2.3 lays the form out "as 17f's .instance-label-form (D3)": the label
  // above the box (not inline against it, where the focus ring covered the
  // last letter), the form --space-3 under the row it follows (DESIGN.md
  // principle 9), Save and Cancel --space-2 apart, and the box at the form's
  // width so the open form does not scroll sideways at 320 px and 200 % text
  // (17f2.9, WCAG 1.4.10). The coder names the class, as brief 21 TP1 review
  // D1 names the card's row items.
  const names = new Set<string>()
  for (const r of RULES)
    for (const m of r.sel.matchAll(/\.(instance-(?:name|label)[-a-z0-9]*)/g))
      names.add(m[1]!)
  // The form's class is the one of the namespace whose rule stacks its
  // children in a column: the title row of 17f2.9 is styled by a class of the
  // same namespace, so the shape is what tells the two apart.
  const stacking = [...names].filter((cls) => {
    const body = topBy(`.${cls}`, `the rule of .${cls}`).body
    const display = norm(prop(body, 'display'))
    return (
      display === 'grid' ||
      (display === 'flex' && norm(prop(body, 'flex-direction')) === 'column')
    )
  })
  expect(
    stacking,
    'the display name form is styled by exactly one class of the slice namespace, the one the markup carries (pinned in InstanceNameEdit17f2.test.tsx)',
  ).toHaveLength(1)
  const cls = stacking[0]!
  const rule = topBy(`.${cls}`, 'the display name form rule')
  expect(
    norm(prop(rule.body, 'margin-top')),
    'the form starts --space-3 under the title row (DESIGN.md principle 9)',
  ).toBe('var(--space-3)')
  const display = norm(prop(rule.body, 'display'))
  expect(
    display === 'grid' ||
      (display === 'flex' &&
        norm(prop(rule.body, 'flex-direction')) === 'column'),
    `the form stacks its children in one column, so the box takes the form's width (17f2.3) and the label goes above it: display flex with flex-direction column, or grid. Today: "${display}"`,
  ).toBe(true)
  const scoped = TOP.filter((r) => r.compact.includes(`.${cls}`))
  expect(
    scoped.some((r) => norm(prop(r.body, 'display')) === 'block'),
    'the label is a block above the box, with the --space-1 under it the brief gives it (today it sits inline against the box)',
  ).toBe(true)
  expect(
    scoped.some((r) => norm(prop(r.body, 'gap')) === 'var(--space-2)'),
    'the two buttons are a row with --space-2 between them (today they touch)',
  ).toBe(true)
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

test('21 TP1 review D1 (as its D4 re-check of 2026-11-23 and 21 T.22 of TP1b amend it): the Tensor item .ot-tensor declares flex: 2 1 18rem, the Goes on item .ot-goes-on flex: 2 1 16rem and the Pattern item .ot-pattern flex-basis: 100%, each with min-width: 0, so at 320 px the items wrap inside the li and never between a label and its control (the TP1b review adds the flex guard)', () => {
  // 21 T.4: the rule's controls are a wrapping flex row and each visible label
  // and its control is ONE item, Tensor asking the widest share, Goes on the
  // next, Pattern a line of its own. With no rule in the sheet the items take
  // their content width and the pairs break apart: at 320 px the "Pattern"
  // label ends beside
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
  // TP1b (21 T.22, the user: "the selects cut their text") retargets the two
  // shares: Tensor 1 1 16rem becomes 2 1 18rem, Goes on 1 1 12rem becomes
  // 2 1 16rem; the Layers item's fixed 8rem and the controls' width: 100% are
  // pinned by 21 T.22 in TensorPlacement3. The items are columns now
  // (21 T.19, pinned by TensorPlacement3), so the Pattern rule's set gains the
  // column declarations, and the selects ask their width (100%), not a basis.
  // The TP1b review (the reviewer, 2026-11-23) keeps this guard and widens
  // it: a rule declaring only flex and min-width (the D2 shrink of the Layers
  // item, `flex: 0 1 8rem; min-width: 0`, written as a rule of its own) is a
  // size, so it names the card too.
  const sameSet = (got: string[], want: string[]) =>
    got.length === want.length &&
    got.sort().every((n, i) => n === want.sort()[i])
  const declaring = (names: string[]) =>
    TOP.filter((r) => sameSet(declared(r.body), names))
  for (const [sel, name, value, what] of [
    [
      '.ot-tensor',
      'flex',
      '2 1 18rem',
      'the Tensor item of 21 T.4 as 21 T.22 widens it (a select of 17 options)',
    ],
    [
      '.ot-goes-on',
      'flex',
      '2 1 16rem',
      'the Goes on item of 21 T.4 as 21 T.22 widens it (a select of devices)',
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
  for (const names of [
    ['width', 'min-width'],
    ['flex', 'min-width'],
  ] as Array<string[]>)
    for (const r of declaring(names))
      for (const p of r.flat.split(','))
        expect(
          p.trim(),
          `a rule declaring ${names.join(' and ')} and nothing else sizes a control: the two selects of 21 T.22 (width: 100%, min-width: 0) and the Layers shrink of the TP1b review D2 (flex: 0 1 8rem, min-width: 0), all of the Tensor placement card: every selector of such a rule names the card (R1.8)`,
        ).toMatch(/^\.ot-/)
  const basisItems = declaring([
    'display',
    'flex-direction',
    'gap',
    'flex-basis',
    'min-width',
  ])
  expect(
    basisItems.length,
    'exactly one top-level rule declares flex-basis and min-width with the 21 T.19 column declarations and nothing else: the Pattern item, on a line of its own (21 T.4, the review D1, 21 T.19)',
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

// ── 22 22C2 review (the designer, 2026-11-26): A1 the quants one per line,
//    A2 the radio and its text ──────────────────────────────────────────────

// A label token in a selector: the word `label` as an element tag, not the
// tail of `.field-label` or `.hf-quant-label`.
const labelTag = (flat: string): boolean =>
  /(^|[\s>+(,])label($|[\s>+:,])/.test(flat)

test('22 22C2 review A1: one top-level rule of the quants namespace lays the quant labels one per line (flex-basis: 100% on the label, or flex-direction: column on their control), the --space-2 row gap that spaces the labels stays, and no full-width rule reaches the Start from labels', () => {
  // The quants fieldset (22 P.9) is a wrapping flex `.field-control`, so today
  // the 12 radios flow several per line in uneven rows (the adviser note the
  // review takes up). A1 wants one per line at every width: each label takes a
  // row of its own (the `.tag-options` basis of 15 C2 review D1) or the
  // control stacks (the `.stacked` direction), and the 8 px between two labels
  // is the field control's own row gap, already --space-2. The review names
  // the rule, not the class (brief 22 section 6): the coder names it in the
  // hf- namespace, the field's strings keys. What jsdom cannot see, the left
  // edges and the 24 px height, is for the UX check.
  const placing = TOP.filter(
    (r) =>
      /\.hf[-a-z0-9]*/.test(r.flat) &&
      (norm(prop(r.body, 'flex-basis')) === '100%' ||
        norm(prop(r.body, 'flex-direction')) === 'column'),
  )
  expect(
    placing.map((r) => r.flat),
    'A1: one top-level rule of the quants namespace (an hf- class) puts one quant label per line: flex-basis: 100% on the label, or flex-direction: column on the control. Today none: the labels flow several per line',
  ).not.toEqual([])
  expect(
    norm(prop(topBy('.field-control', 'the field control').body, 'gap')).split(
      /\s+/,
    )[0],
    "A1: two quant labels are 8 px apart: the field control's row gap stays --space-2 (a new gap on the quants that drops it is the other way; the row gap is the sheet's shape today)",
  ).toBe('var(--space-2)')
  // The Start from radios keep their layout (A1, "the other radios"): a
  // full-width rule naming a bare label would stack them one per line too.
  for (const r of TOP.filter(
    (r) => norm(prop(r.body, 'flex-basis')) === '100%' && labelTag(r.flat),
  ))
    expect(
      r.flat,
      'A1: the Start from radios wrap in a row as now: a full-width rule naming a bare label stacks them too, so it must name a quants class',
    ).toMatch(/\.hf[-a-z0-9]*/)
})

test('22 22C2 review A2: a rule makes every radio label a box (display: flex or grid) with 8 px (--space-2) between the radio and its text (gap on the label, or padding/margin on the radio), scoped so it reaches no checkbox label, and a --space-2 gap on a label names the field control or the radio', () => {
  // Today the radio sits against its text (the adviser note, seen on the
  // screenshot): no rule in the sheet styles the labels of a `.field-control`.
  // A2 wants 8 px inside every radio label, as padding or flex gap, the label
  // staying one native click target (the click half is jsdom-visible and
  // pinned in ModelProfilesNew5; the gap measurement is for the UX check).
  // The box is also what A1's long labels need: the text is an item beside
  // the input, so every line of a wrapped label keeps to the right of the
  // radio, never under it. Scope: the two fieldsets of NewModelProfile.tsx
  // today, 22E's picker inherits; checkboxes and switches (the `.check`
  // rows, the switches) keep their own layout (fe/DESIGN.md 5.2).
  const inScope = (r: Rule) =>
    labelTag(r.flat) &&
    (r.flat.includes('.field-control') ||
      /input\[type=['"]?radio['"]?\]/.test(r.flat))
  const boxed = TOP.filter((r) =>
    inScope(r)
      ? ['flex', 'inline-flex', 'grid', 'inline-grid'].includes(
          norm(prop(r.body, 'display')),
        )
      : false,
  )
  expect(
    boxed.map((r) => r.flat),
    'A2 (and A1 long labels): a top-level rule names a radio label (scoped to .field-control, or :has a radio input) and makes it a box (display: flex/grid), so the text sits beside the radio and a wrapped label wraps to the right of it. Today none',
  ).not.toEqual([])
  const padded = TOP.some(
    (r) =>
      inScope(r) &&
      /input\[type=['"]?radio['"]?\]/.test(r.flat) &&
      [
        'padding-left',
        'padding-inline-start',
        'margin-left',
        'margin-inline-start',
      ].some((n) => norm(prop(r.body, n)) === 'var(--space-2)'),
  )
  expect(
    boxed.some((r) => norm(prop(r.body, 'gap')) === 'var(--space-2)') || padded,
    'A2: the 8 px between the radio and its text: gap: var(--space-2) on the label box, or padding-left / margin-left: var(--space-2) on the radio inside the label',
  ).toBe(true)
  for (const r of TOP.filter(
    (r) => labelTag(r.flat) && norm(prop(r.body, 'gap')) === 'var(--space-2)',
  ))
    expect(
      r.flat,
      "A2: checkboxes and switches are out of scope: a --space-2 gap on a label that names neither .field-control nor a radio input reaches the GPU list's .check rows and the switches, whose rows keep their own layout (fe/DESIGN.md 5.2)",
    ).toMatch(/\.field-control|input\[type=['"]?radio['"]?\]/)
})

// ── 22 22C2 re-check (the designer, 2026-11-26): D2 the browser's own margin
//    on the radio, and the adviser note: the radio on the label's first line ─

// A rule that reaches a radio's label, the A2 scope: the label of a
// `.field-control`, or a label naming a radio input.
const radioLabel = (r: Rule): boolean =>
  labelTag(r.flat) &&
  (r.flat.includes('.field-control') || /radio/.test(r.flat))

// A rule whose subject is the radio input itself. A selector that names the
// radio only inside `:has(...)` styles the LABEL (`:has(> input[type='radio'])`
// is the A2 rule), so a declaration there does not reach the input.
const radioInput = (r: Rule): boolean =>
  /input\[type=['"]?radio['"]?\]/.test(r.flat.replace(/:has\([^()]*\)/g, ''))

test("22 22C2 re-check D2: a top-level rule whose subject is the radio input zeroes the browser's own margin (margin: 0, the shorthand or the four sides), and it reaches a radio of every field control, so the label measures the 8 px gap alone", () => {
  // The gap is on the label (the A2 rule, index.css 1553-1558: `gap:
  // var(--space-2)`), so the label should measure 8 px between the input's
  // right edge and the text's left. It measures 11 px in Chromium and Firefox
  // and 10 in WebKit, because the browser's own margin on the radio (Chromium's
  // UA `margin: 3px 3px 0 5px`) adds to it. The sheet's `*` reset carries
  // box-sizing alone (index.css 261), so nothing zeroes the radio's today; the
  // drawn checkbox is already handled that way (`input[type='checkbox'] {
  // margin: 0 }`, index.css 1794), the sheet's precedent for the fix.
  const zeroed = TOP.filter(
    (r) =>
      radioInput(r) &&
      (norm(prop(r.body, 'margin')) === '0' ||
        ['top', 'right', 'bottom', 'left'].every(
          (side) => norm(prop(r.body, `margin-${side}`)) === '0',
        )),
  )
  expect(
    zeroed.map((r) => r.flat),
    "D2: a top-level rule whose subject is input[type='radio'] declares margin: 0 (the shorthand, or the four sides at 0). Today none names a radio, so the browser's 5 px left margin and 3 px top margin add to the label's gap and the text starts 10 to 11 px after the radio",
  ).not.toEqual([])
  // Both radio groups are affected (D2), and both sit in a `.field-control`
  // (NewModelProfile.tsx 840 for "Start from", 1029 for the quants), so a rule
  // scoped to a quants class fixes one group and leaves the other at 11 px.
  for (const r of zeroed)
    expect(
      r.flat,
      'D2: both radio groups are affected: a rule scoped to a quants class (.hf-, the quants fieldset alone) leaves the "Start from" radio with its browser margin; `.field-control input[type=\'radio\']` reaches both',
    ).not.toMatch(/\.hf[-a-z0-9]*/)
})

test("22 22C2 re-check (adviser note a): the radio sits on its label's first line: the radio label box aligns its items at the start (align-items: start on the box, or align-self: start on the radio, the sheet shapes), the box no longer centres them, and the radio carries a first-line offset (a margin-top or padding-top on the radio)", () => {
  // A label too long for its line is two lines, and the A2 grid centres its
  // items on the whole box (`align-items: center`, index.css 1556), so the
  // radio sits between the two lines, seen at 320 px on "Q4_K_M, 2.2 GiB,
  // llama.cpp's default". The usual pattern puts it on the first line: the box
  // aligns at the start and the radio carries the rest, half a line over its
  // own box, the way `.notice > .icon` does (index.css 1018 `margin-top:
  // calc((1lh - 1.25em) / 2)`). The measurement itself, the radio's centre the
  // centre of the text's first line give or take 2 px, is for the UX check:
  // jsdom lays out nothing.
  const atStart = ['start', 'flex-start', 'grid-start']
  expect(
    TOP.some(
      (r) =>
        radioLabel(r) && atStart.includes(norm(prop(r.body, 'align-items'))),
    ) ||
      TOP.some(
        (r) =>
          radioInput(r) && atStart.includes(norm(prop(r.body, 'align-self'))),
      ),
    "Adviser note (a): the radio is aligned at the start of its label's line, one of the two sheet shapes: align-items: start on the radio label box (the A2 rule), or align-self: start on the radio input. Today neither: the box says align-items: center",
  ).toBe(true)
  expect(
    TOP.filter(
      (r) =>
        radioInput(r) &&
        ['margin-top', 'padding-top'].some((name) => {
          const v = norm(prop(r.body, name))
          return v !== '' && v !== '0' && v !== 'auto'
        }),
    ).map((r) => r.flat),
    "Adviser note (a): the radio carries the first-line offset that centres it on the first line (a margin-top, as the notice's icon at index.css 1018, or a padding-top): aligning at the start alone leaves the radio's centre half the leading above the line's centre, over the 2 px the note gives. Today no rule names a radio at all",
  ).not.toEqual([])
  // The centring that sits the radio between the lines goes, in one of the two
  // shapes: the box stops declaring it, or the radio overrides it for itself
  // (`align-self` wins over `align-items`, CSS Grid 8.3).
  const selfStart = TOP.some(
    (r) => radioInput(r) && atStart.includes(norm(prop(r.body, 'align-self'))),
  )
  for (const r of TOP.filter(
    (r) => radioLabel(r) && norm(prop(r.body, 'align-items')) === 'center',
  ))
    expect(
      selfStart,
      `${r.flat}: adviser note (a): a radio label box that still centres its items is the unmet case (index.css 1556 \`align-items: center\`, the declaration this note takes away): the fix is align-items: start on the box, or align-self: start on the radio, which overrides it for the radio alone`,
    ).toBe(true)
  // The fix is inside the label, not on the control row: `.field-control` is
  // the wrapping row of every field, and its `align-items: center` centres the
  // selects, the checkboxes and the switches in it (out of scope, DESIGN.md
  // 5.2), so it keeps its centring.
  expect(
    norm(
      prop(topBy('.field-control', 'the field control').body, 'align-items'),
    ),
    'Adviser note (a): the field control keeps align-items: center: the fix belongs to the radio label, not to the row that centres every control of every field',
  ).toBe('center')
})

// ── Brief 22, slice L2: 22 CD.2 (the destructive verb filled in danger),
// 22 L2.B1 (the Save bar sticky at the bottom edge) and 22 D4.35 (the Models
// list Engine column folded under the name below 480 px).
// The DOM half of the bar's look (22 D4.30 with L2.A2: the Delete pushed to the
// bar end and drawn --danger) is pinned against the rendered pages in
// L2SaveBar.test.tsx, which asks the sheet for the rules that reach the control
// and names no class of its own; the CD.1 verbs, the D4.31 card intros and the
// D4.32 head are DOM too, in L2SaveBar.test.tsx and L2PageHead.test.tsx, and so
// are L2.B1's place (the bar the settings section's last child, its three stops
// the page's last) and L2.B2's reason (a floating tip, not a line of the bar).
// What jsdom cannot see is the designer's, of the UX check's page states: the
// last card's bottom lying at or above the bar's top when the page is scrolled
// to its end (L2.B1 c), the bar at most 40 % of a 320 by 568 viewport with no
// sideways scroll (L2.B1 e), the tip wholly inside the viewport at 1280 and
// 320 px (L2.B2 c), the D4.35 table needing no sideways scroll inside its box at
// 320 px with its trash can inside the box's right edge, the D4.32 gaps and the
// D4.33 hint line.
//
// The theme tokens and the WCAG contrast arithmetic are a small local copy of
// HearthTokens.test.ts, which exports nothing and whose tests would register
// here if it were imported.

// The custom properties of a block, in the order it declares them.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[\w-]+)\s*:\s*([^;]+)/g)].map((m) => [
    m[1]!,
    m[2]!.trim(),
  ])

// The tokens of the light theme (the top-level :root) and of the dark one
// (the :root:not([data-theme='light']) rule of the prefers-color-scheme: dark
// block, 08 H4.10).
const lightTokens = (): [string, string][] =>
  customProps(topBy(':root', 'the :root token block').body)
const darkTokens = (): [string, string][] => {
  const [open, end] = atRanges('@media (prefers-color-scheme: dark)')[0]!
  return customProps(
    single(
      RULES.filter(
        (r) =>
          r.depth > 0 &&
          r.at > open &&
          r.at < end &&
          r.compact === ":root:not([data-theme='light'])",
      ),
      "the :root:not([data-theme='light']) rule of the dark media block",
    ).body,
  )
}

// The value of one token; a miss throws naming it, so a contrast pair that
// lacks a token goes red on the token, not on NaN.
const valueOf = (pairs: [string, string][], name: string): string => {
  const hits = pairs.filter((p) => p[0] === name).map((p) => p[1])
  if (hits.length !== 1)
    throw new Error(`${name} is declared ${hits.length} times in the block`)
  return hits[0]!
}

// WCAG 2.2 relative luminance and contrast ratio (the 0.04045 threshold), from
// the hex values read from the sheet.
const rgbOf = (hex: string): [number, number, number] => {
  const m = /^#([0-9a-fA-F]{6})$/.exec(hex)
  if (!m) throw new Error(`not a 6-digit hex colour: "${hex}"`)
  return [
    parseInt(m[1].slice(0, 2), 16),
    parseInt(m[1].slice(2, 4), 16),
    parseInt(m[1].slice(4, 6), 16),
  ]
}
const chan = (v: number): number => {
  const c = v / 255
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}
const luminance = (hex: string): number => {
  const [r, g, b] = rgbOf(hex)
  return 0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b)
}
const contrast = (a: string, b: string): number => {
  const ls = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (ls[0]! + 0.05) / (ls[1]! + 0.05)
}

// The selector parts of a rule, one string each.
const parts = (r: Rule): string[] => r.flat.split(',').map((p) => p.trim())
// The Models table's second column: its body cell, and its header. The column
// is named by its place, as the sheet names it today (index.css 820), so no
// class the coder gives it is pinned here.
const engineCell = (p: string): boolean =>
  p.includes('.model-profiles') &&
  /\btd\b/.test(p) &&
  p.includes('nth-child(2)')
const engineHead = (p: string): boolean =>
  p.includes('.model-profiles') &&
  /\bth\b/.test(p) &&
  p.includes('nth-child(2)')
// The rules of every @media (max-width: 479.98px) block: the sheet's very
// narrow block (index.css 660), where a column folds.
const narrow = (): Rule[] => {
  const ranges = atRanges('@media (max-width: 479.98px)')
  expect(
    ranges.length,
    'the sheet has the @media (max-width: 479.98px) block of index.css 660, where 22 D4.35 folds the Engine column',
  ).toBeGreaterThan(0)
  return RULES.filter((r) =>
    ranges.some(([open, end]) => r.depth > 0 && r.at > open && r.at < end),
  )
}

test('22 CD.2: .primary.danger fills with --danger and writes in --on-accent, is written before the shared aria-disabled rule, and its pair clears 4.5:1 in both themes', () => {
  const danger = topBy(
    '.primary.danger',
    'the rule of the destructive verb (22 CD.1 gives the class)',
  )
  declares(
    danger,
    '22 CD.2',
    [
      ['background', 'var(--danger)'],
      ['border-color', 'var(--danger)'],
      ['color', 'var(--on-accent)'],
    ],
    'the three declarations CD.2 gives, no new token and no fourth: the shared .primary rule (index.css 377) draws the pill, the padding and the weight, and this rule only changes the pair',
  )
  // The order: a switched-off verb keeps the shared inactive look, so the
  // inactive rule is written after this one (CD.2: "the rule sits before
  // .primary[aria-disabled='true']").
  const inactive = TOP.filter((r) =>
    parts(r).some((p) => p === ".primary[aria-disabled='true']"),
  )
  expect(
    inactive.length,
    'the shared aria-disabled rule of index.css 2647 names .primary: CD.2 says the danger rule sits before it',
  ).toBeGreaterThan(0)
  for (const r of inactive)
    expect(
      danger.at,
      `"${r.flat}": 22 CD.2 writes .primary.danger before the aria-disabled rule, so the Remove of an instance that runs again (16 17c.7) keeps the inactive look`,
    ).toBeLessThan(r.at)
  // The pair, from the tokens as the sheet writes them. The brief's own
  // arithmetic gives 6.5:1 light and 8.7:1 dark; the pin is the floor of
  // WCAG 1.4.3, which is what the rule has to clear.
  for (const [theme, tokens] of [
    ['light', lightTokens()],
    ['dark', darkTokens()],
  ] as const) {
    const ink = valueOf(tokens, '--on-accent')
    const fill = valueOf(tokens, '--danger')
    expect(
      contrast(ink, fill),
      `22 CD.2, ${theme} theme: ${ink} on ${fill} is the text of the verb that destroys, and clears the 4.5:1 of WCAG 1.4.3`,
    ).toBeGreaterThanOrEqual(4.5)
  }
})

// L2.B1 b: the bar's box, flipped to the bottom edge.
test('22 L2.B1: the sheet sticks the Save bar to the bottom edge: .save-bar is sticky at bottom 0 with no top, its edge and its round corners on the top, and its background, z-index and shadow kept', () => {
  const bar = topBy('.save-bar', 'the bar rule')
  expect(prop(bar.body, 'position'), '22 L2.B1 b: sticky').toBe('sticky')
  expect(prop(bar.body, 'bottom'), 'at the viewport’s bottom edge').toBe('0')
  expect(
    prop(bar.body, 'top'),
    '22 L2.B1 b reverses 22 D2.6’s top: no top edge is stuck, which would leave the bar in flow at the top and floating at the bottom',
  ).toBe('')
  expect(
    prop(bar.body, 'z-index'),
    'the layer is the same as today (L2.B1 b: the same z-index)',
  ).toBe('1')
  expect(
    prop(bar.body, 'background'),
    'the same fill as the card it sits over (H1a.8, kept)',
  ).toBe('var(--bg)')
  expect(prop(bar.body, 'box-shadow'), 'the lift is kept').toBe('var(--shadow)')
  // The edge and the corners follow the bar: a top border replaces the bottom
  // one, and only the top corners are round.
  expect(
    prop(bar.body, 'border-top'),
    'L2.B1 b: the coloured 1px --control edge moves to the top (the cue that survives forced colours, H2b.4)',
  ).toBe('1px solid var(--control)')
  expect(
    prop(bar.body, 'border-bottom'),
    'and leaves the bottom edge, which is flush',
  ).toBe('')
  expect(
    prop(bar.body, 'border-radius'),
    'L2.B1 b: the card corner on the two top corners only',
  ).toBe('var(--radius-card) var(--radius-card) 0 0')
})

// L2.B1 d: the focus-not-obscured pair, moved to the bottom edge.
test('22 L2.B1: the page pads its bottom edge for the bar and the bar’s controls cancel that padding: html:has(.save-bar) declares scroll-padding-bottom with the 7rem floor, .save-bar button the equal negative scroll-margin-bottom, and the top pair is gone', () => {
  // The same calc the top padding has today (2h.135): the pin is the value as
  // written, Prettier and a reflow of the calc aside.
  const PAD = 'calc(max(var(--save-bar-height, 7rem), 7rem) + var(--space-3))'
  const pad = topBy('html:has(.save-bar)', 'the page padding rule')
  expect(
    norm(prop(pad.body, 'scroll-padding-bottom')),
    '22 L2.B1 d: the padding moves to the bottom edge, the same value',
  ).toBe(PAD)
  expect(
    prop(pad.body, 'scroll-padding-top'),
    '22 L2.B1 d: nothing sticks to the top edge any more, so the page pads nothing there (WCAG 2.4.11)',
  ).toBe('')
  const btn = topBy('.save-bar button', 'the bar’s controls rule')
  // The margin's calc multiplies the padding's inner expression by -1, the same
  // reading as HearthSaveBarScroll.test.tsx's H5c.1: nesting the padding's own
  // calc inside would ask for calc(-1 * (calc(...))), which no sheet writes.
  const inner = PAD.replace(/^calc\((.*)\)$/, '$1')
  expect(
    norm(prop(btn.body, 'scroll-margin-bottom')),
    '22 L2.B1 d: a control of the bar cancels the padding with the equal negative margin, at the bottom edge (H5c.1, now at the bottom)',
  ).toBe(`calc(-1 * (${inner}))`)
  expect(
    prop(btn.body, 'scroll-margin-top'),
    '22 L2.B1 d: the top cancel goes with the top padding, so a focused bar control is not pushed off the top of the viewport (H5c.2)',
  ).toBe('')
})

// L2.B1 e: the short-viewport rule, flipped the same way.
test('22 L2.B1: the max-height 31.98rem rule is flipped too: the bar is static, at the form’s end, and the bottom padding and margin are 0 (WCAG 1.4.10)', () => {
  const ranges = atRanges('@media (max-height: 31.98rem)')
  expect(
    ranges.length,
    'the sheet keeps the short-viewport block of 2h.135 (L2.B1 e: it stays)',
  ).toBeGreaterThan(0)
  const short = RULES.filter((r) =>
    ranges.some(([open, end]) => r.depth > 0 && r.at > open && r.at < end),
  )
  expect(
    prop(
      single(
        short.filter((r) => r.compact === '.save-bar'),
        'the .save-bar rule inside the short-viewport block',
      ).body,
      'position',
    ),
    '22 L2.B1 e: on a short viewport the bar is static, so it rests at the form’s end',
  ).toBe('static')
  expect(
    norm(
      prop(
        single(
          short.filter((r) => r.compact === 'html:has(.save-bar)'),
          'the html:has(.save-bar) rule inside the short-viewport block',
        ).body,
        'scroll-padding-bottom',
      ),
    ),
    '22 L2.B1 e: the bottom padding is zeroed there',
  ).toBe('0')
  expect(
    norm(
      prop(
        single(
          short.filter((r) => r.compact === '.save-barbutton'),
          'the .save-bar button rule inside the short-viewport block',
        ).body,
        'scroll-margin-bottom',
      ),
    ),
    '22 L2.B1 e: the cancel is zeroed with it, so a focused bar control is not pulled past the viewport',
  ).toBe('0')
  // The top pair, gone from the block as it is from the top level.
  for (const r of short) {
    expect(
      prop(r.body, 'scroll-padding-top'),
      `"${r.flat}": L2.B1 e leaves no top padding of the short viewport behind, where nothing sticks any more`,
    ).toBe('')
    expect(
      prop(r.body, 'scroll-margin-top'),
      `"${r.flat}": L2.B1 e leaves no top cancel of the short viewport behind`,
    ).toBe('')
  }
})

// L2.B2 c: the reason of a page Delete that cannot act is the list's floating
// tip, so it takes no room in the bar.
test('22 L2.B2: the reason of a page Delete that cannot act floats over the page: .hint-tip is position: fixed on a layer above the Save bar', () => {
  const tip = topBy('.hint-tip', 'the tip rule of 22 D4.16')
  expect(
    prop(tip.body, 'position'),
    '22 L2.B2 c: the bar’s Delete uses the list rows’ component, whose tip is out of the flow, so showing it changes no size in the bar',
  ).toBe('fixed')
  expect(
    Number(prop(tip.body, 'z-index')),
    '22 L2.B2 c: the tip is above the sticky bar, which sits on layer 1, so the reason of a Delete at the viewport’s bottom edge is never behind the bar',
  ).toBeGreaterThan(
    Number(prop(topBy('.save-bar', 'the bar rule').body, 'z-index')),
  )
})

test('22 D4.35: below 480 px the Models list hides the words of the Engine column and keeps its header in the tree, and nothing hides that column at 480 px and wider', () => {
  const inNarrow = narrow()
  // The column is not shown: the second cell of every row is taken out.
  expect(
    inNarrow
      .filter(
        (r) =>
          parts(r).some(engineCell) && norm(prop(r.body, 'display')) === 'none',
      )
      .map((r) => r.flat),
    '22 D4.35: a rule of the @media (max-width: 479.98px) block hides the Engine cell of every row of the Models table with display: none, so its words are not shown below 480 px (the row keeps its second cell, whose place the name cell fills with the muted line: L2ModelsList.test.tsx pins the DOM)',
  ).not.toEqual([])
  // The header: hidden from the eye, kept in the tree. The shape is the
  // sheet's own .visually-hidden one (index.css 350), which a media rule
  // writes because a class toggled by width would need script (the sheet's own
  // note above index.css 658). clip-path and position are two of its
  // declarations, the ones no other rule of the sheet pairs.
  expect(
    inNarrow
      .filter(
        (r) =>
          parts(r).some(engineHead) &&
          norm(prop(r.body, 'position')) === 'absolute' &&
          norm(prop(r.body, 'clip-path')) === 'inset(50%)',
      )
      .map((r) => r.flat),
    '22 D4.35: a rule of the same block hides the Engine header the way .visually-hidden hides (position: absolute, clip-path: inset(50%)), so the header is out of the eye and in the accessibility tree, and its sort button stays reachable',
  ).not.toEqual([])
  // The trap: display: none on the header takes the button out of the tree,
  // and the column can no longer be sorted. No rule of the sheet, in any
  // block, does it.
  for (const r of RULES)
    for (const p of parts(r))
      if (engineHead(p))
        expect(
          norm(prop(r.body, 'display')),
          `"${r.flat}": 22 D4.35 keeps the Engine header reachable, so the header is never display: none (CSS Display: the whole subtree leaves the accessibility tree, and the sort button with it)`,
        ).not.toBe('none')
  // At 480 px and wider nothing changes (the third item of D4.35): the column
  // is hidden nowhere but in a max-width block.
  for (const r of RULES)
    if (parts(r).some(engineCell) && norm(prop(r.body, 'display')) === 'none')
      expect(
        r.depth,
        `"${r.flat}": 22 D4.35 hides the Engine column below 480 px only, so the rule sits in a @media block and not at the top level, where it would hide the column at every width`,
      ).toBeGreaterThan(0)
})

test('22 L2 D4.32 and D4.33 (the sheet): the page switch and its word are --space-2 apart, the state line leaves --space-5 before the first card, and the filter sits --space-4 above the table', () => {
  // The three gaps the brief gives in tokens, each pinned as "some top-level
  // rule of this subject declares this token in one of the spacing
  // properties": the property (gap, a margin, the shorthand) is the coder's
  // choice, the token and the subject are the brief's.
  const SPACING = [
    'gap',
    'row-gap',
    'margin',
    'margin-top',
    'margin-bottom',
    'margin-inline-start',
    'margin-inline-end',
  ]
  const spaced = (subject: string, token: string): string[] =>
    TOP.filter((r) => r.compact.includes(subject))
      .filter((r) =>
        SPACING.some((p) => norm(prop(r.body, p)).includes(`var(${token})`)),
      )
      .map((r) => r.flat)
  for (const [subject, token, what] of [
    [
      '.model-profiles-switch',
      '--space-2',
      "22 D4.32: the word sits --space-2 beside the page switch, so it reads as the switch's, not as the next line's",
    ],
    [
      '.model-profile-summary',
      '--space-5',
      '22 D4.32: the state line is followed by --space-5 before the first card, so the eye reads the state with the title and the cards as the form',
    ],
    [
      '.model-profiles-filter',
      '--space-4',
      '22 D4.33: the filter sits --space-4 above the table, so the filter is not read as a row of it',
    ],
  ] as const)
    expect(
      spaced(subject, token),
      `22 L2: a top-level rule of ${subject} spaces it by var(${token}) (${what})`,
    ).not.toEqual([])
})
