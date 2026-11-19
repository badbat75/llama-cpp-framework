// Hearth H9a, the model profile page's jump list drawn as an index
// (fe/briefs/08-hearth-h9-jump.md, section 1): the stylesheet half. H9a is CSS
// only, so these tests read index.css as text, the way HearthH2a.test.ts and
// DisabledLook.test.ts do (comments dropped, rules from the brace balance, the
// nesting depth from the braces before the selector, prop() the last value of a
// property in a block); a test file imports no other test file, so the reader
// here is a small local copy of theirs.
//
// What the text cannot pin goes to the designer's UX pass (section 3): the
// index column under the h2 at 320 px, the absence of sideways scroll, the
// focus ring whole at the rounded right end (H9a.4), and that a long title
// reflows inside its entry (H9a.6): jsdom does no layout.
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
  // Whitespace dropped, so a selector can be named without its space.
  compact: m[1].replace(/\s+/g, ''),
  // Runs of whitespace become one space: how the sheet writes a descendant.
  flat: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the order of the rules inside an @media block says
// nothing about the cascade of the top-level ones.
const TOP = RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
// Runs of whitespace collapse to one space, because Prettier re-wraps a long
// value and an exact compare then reds on the wrap.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .trim()
        .replace(/\s+/g, ' ')
  }
  return val
}

// The properties a block declares, sorted: a set equality is what refuses a
// property the brief does not list.
const decls = (body: string): string[] =>
  body
    .split(';')
    .filter((d) => d.includes(':'))
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .sort()

// A throw names the miss, so "the rule is missing" never reads as a typo.
const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length ? ` (${list.map((r) => r.flat).join(' | ')})` : ''),
    )
  return list[0]
}

// index.css is written with no space between a compound's parts, so a
// descendant selector is handed to the compact matcher with its space dropped.
const tight = (sel: string): string => sel.replace(/ /g, '')

const topBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === tight(sel)),
    `the top-level rule whose selector is exactly ${sel} (${what})`,
  )

// The rules of every top-level block written with this at-rule header whose
// selector holds `has`.
const inside = (needle: string, has: string): Rule[] => {
  const hits: Rule[] = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(needle, from)
    if (at < 0) return hits
    let end = at
    let depth = 0
    for (; end < CSS.length; end++) {
      if (CSS[end] === '{') depth++
      else if (CSS[end] === '}' && --depth === 0) break
    }
    for (const r of RULES)
      if (r.depth > 0 && r.at > at && r.at < end && r.compact.includes(has))
        hits.push(r)
    from = end + 1
  }
}

// The items of a selector list, split on the commas outside brackets.
const listItems = (sel: string): string[] => {
  const items: string[] = []
  let cur = ''
  let depth = 0
  for (const ch of sel) {
    if (ch === '(' || ch === '[') depth++
    else if (ch === ')' || ch === ']') depth--
    if (ch === ',' && depth === 0) {
      items.push(cur)
      cur = ''
    } else cur += ch
  }
  items.push(cur)
  return items.map((s) => s.replace(/\s+/g, ' ').trim()).filter((s) => s !== '')
}

// A rule whose selector list names this selector whole.
const selIs = (sel: string) => (r: Rule) =>
  listItems(r.sel).some((s) => s === sel)

const FORCED = '@media (forced-colors: active)'

// ── H9a.2: the list is a column, with a rule at its left ───────────────────

test('H9a.2: .jump ul is a block list: no flex, the --space-2 and --space-4 margins, no padding, no list markers, a decorative 2px --line border at its left', () => {
  const rule = topBy('.jump ul', 'the jump list rule (H9a.2)')
  expect(
    prop(rule.body, 'display'),
    'a column of entries: block, so no row wraps (H9a.2)',
  ).toBe('block')
  expect(
    prop(rule.body, 'margin'),
    'the space the list takes from the h2 and leaves to the notice',
  ).toBe('var(--space-2) 0 var(--space-4)')
  expect(prop(rule.body, 'padding'), 'the list box carries no padding').toBe(
    '0',
  )
  expect(
    prop(rule.body, 'list-style'),
    'no markers: the entries are told apart by their text',
  ).toBe('none')
  expect(
    prop(rule.body, 'border-left'),
    'the thin rule at the left of the index, the decorative --line (brief 04 criterion 4)',
  ).toBe('2px solid var(--line)')
  expect(
    decls(rule.body),
    'H9a.2 lists these properties and no other: with display: block the flex properties and the row gap of the pill row are dead weight',
  ).toEqual(['border-left', 'display', 'list-style', 'margin', 'padding'])
})

// ── H9a.3: the entry, one rule ─────────────────────────────────────────────

test('H9a.3: one top-level .jump button rule, the entry: block, full width, a 32px min-height, --space-1 and --space-3 padding, no border, the pill corner at the right, transparent on --accent, --fs-md, left aligned, underlined 1px at 3px', () => {
  const jump = TOP.filter(selIs('.jump button'))
  expect(
    jump.length,
    'exactly one top-level .jump button rule: H9a.3 replaces the min-height rule of H2a.1, it does not add a second one',
  ).toBe(1)
  const rule = jump[0]!
  const WANT: [string, string][] = [
    ['display', 'block'],
    ['width', '100%'],
    ['min-height', '32px'],
    ['padding', 'var(--space-1) var(--space-3)'],
    ['border', '0'],
    ['border-radius', '0 var(--radius-pill) var(--radius-pill) 0'],
    ['background', 'transparent'],
    ['color', 'var(--accent)'],
    ['font-size', 'var(--fs-md)'],
    ['text-align', 'left'],
    ['text-decoration', 'underline'],
    ['text-decoration-thickness', '1px'],
    ['text-underline-offset', '3px'],
  ]
  for (const [name, want] of WANT)
    expect(
      prop(rule.body, name),
      `.jump button ${name}: H9a.3 gives ${want}`,
    ).toBe(want)
  expect(
    decls(rule.body).filter((n) => n !== 'overflow-wrap'),
    'H9a.3 lists these properties and no other on the entry (overflow-wrap is H9a.6)',
  ).toEqual(WANT.map(([n]) => n).sort())
})

// ── H9a.5: forced colours ──────────────────────────────────────────────────

test('H9a.5: inside a forced-colours block a .jump button rule sets color: LinkText, and nothing there takes the underline away', () => {
  const rule = single(
    inside(FORCED, tight('.jump button')).filter(selIs('.jump button')),
    'the .jump button rule inside a @media (forced-colors: active) block (H9a.5)',
  )
  expect(
    prop(rule.body, 'color'),
    'with no border the entries read as the links they look like, so they take the link colour of the forced palette',
  ).toBe('LinkText')
  for (const r of inside(FORCED, tight('.jump button')))
    expect(
      prop(r.body, 'text-decoration'),
      `${r.flat}: only the colour is forced, the underline stays (CSS Color Adjustment 1, 3.1)`,
    ).not.toBe('none')
})

// ── H9a.6: reflow, the half the text can pin ───────────────────────────────

test('H9a.6: a long card title wraps inside its entry (overflow-wrap: anywhere), and the entry is sized by a min-height, never a height', () => {
  const rule = topBy('.jump button', 'the entry (H9a.3, H9a.6)')
  expect(
    prop(rule.body, 'overflow-wrap'),
    'a long title breaks inside the entry, so no 320 px page scrolls sideways (1.4.10)',
  ).toBe('anywhere')
  expect(
    prop(rule.body, 'height'),
    'no fixed height: the entry grows with the text it wraps (1.4.12)',
  ).toBe('')
  expect(
    prop(rule.body, 'min-height'),
    'the 32 px target of H9a.3 is a floor, not a box',
  ).toBe('32px')
})
