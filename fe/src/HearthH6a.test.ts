// Hearth H6a, the CSS (fe/briefs/08-hearth-h6-overview.md, H6a.5 to H6a.8 and
// the H6a.10 follow-up): the stylesheet text of the tile, pinned as the
// declarations of the named rules, the way HearthH2b.test.ts reads index.css
// (jsdom does no layout, and importing a test file would register its tests, so
// the reader is a local copy: comments dropped, the same rule regex, the same
// nesting depth, prop() the last value of a property in a block).
//
// The values are the brief's, written as it writes them. Where the brief names
// an absence (`border-radius: 18px`, "No `height`") the absence is pinned too:
// a height is what breaks 1.4.12.
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
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: a rule inside an @media block is not the rule of the
// tile, it is an override of it.
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
    TOP.filter((r) => r.compact === sel.replace(/ /g, '')),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
  )

/** The rule, with `why` for every message: a miss says which rule of H6a. */
const ruleFor = (sel: string, why: string): Rule => topBy(sel, why)

/** Every property a block declares. */
const declared = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .filter((name) => name !== '' && !name.startsWith('--'))

// ── H6a.5: the list ────────────────────────────────────────────────────────

test('H6a.5: .tiles is a grid of tiles that is one column at 320 px, with no list marks', () => {
  const rule = ruleFor('.tiles', 'the list of tiles')
  for (const [name, value] of [
    ['list-style', 'none'],
    ['margin', '0'],
    ['padding', '0'],
    ['display', 'grid'],
    ['gap', 'var(--space-4)'],
    [
      'grid-template-columns',
      'repeat(auto-fill, minmax(min(100%, 15rem), 1fr))',
    ],
  ] as [string, string][])
    expect(prop(rule.body, name), `.tiles: ${name}`).toBe(value)
  // the track never asks for more width than the page has (1.4.10)
  expect(prop(rule.body, 'grid-template-columns')).toContain('min(100%, 15rem)')
})

// ── H6a.6: the tile ────────────────────────────────────────────────────────

test('H6a.6: .tile is the lifted box of the card, a column, with nothing fixing its height', () => {
  const rule = ruleFor('.tile', 'the tile')
  for (const [name, value] of [
    ['background', 'var(--bg)'],
    ['border', '1px solid transparent'],
    ['border-radius', '18px'],
    ['box-shadow', 'var(--shadow)'],
    ['padding', 'var(--space-4) var(--space-5) var(--space-5)'],
    ['display', 'flex'],
    ['flex-direction', 'column'],
    ['gap', 'var(--space-2)'],
  ] as [string, string][])
    expect(prop(rule.body, name), `.tile: ${name}`).toBe(value)
  // the edge is a transparent border: it survives forced colours, as .card's
  expect(prop(rule.body, 'border')).toContain('transparent')
  // 1.4.12: only the content sizes the tile
  expect(declared(rule.body), '.tile declares no height').not.toContain(
    'height',
  )
})

// ── H6a.7: the head row, the id, the engine line ───────────────────────────

test('H6a.7: .tile-head is the row that lets the badge wrap under a long id', () => {
  const rule = ruleFor('.tile-head', 'the head row of the tile')
  for (const [name, value] of [
    ['display', 'flex'],
    ['flex-wrap', 'wrap'],
    ['align-items', 'center'],
    ['justify-content', 'space-between'],
    ['gap', 'var(--space-2)'],
  ] as [string, string][])
    expect(prop(rule.body, name), `.tile-head: ${name}`).toBe(value)
})

test('H6a.7: .tile-id is the id as a title in the accent, an underlined link on hover, free to break', () => {
  const rule = ruleFor('.tile-id', 'the id link')
  for (const [name, value] of [
    ['font-size', '1.25rem'],
    ['font-weight', '700'],
    ['color', 'var(--accent)'],
    ['text-decoration', 'none'],
    ['overflow-wrap', 'anywhere'],
    ['min-width', '0'],
  ] as [string, string][])
    expect(prop(rule.body, name), `.tile-id: ${name}`).toBe(value)
  const hover = ruleFor('.tile-id:hover', 'the id link under the pointer')
  expect(
    prop(hover.body, 'text-decoration'),
    '.tile-id:hover is underlined: the link shows it is a link',
  ).toBe('underline')
})

test('H6a.7: .tile-engine is the quiet line under the head', () => {
  const rule = ruleFor('.tile-engine', 'the engine line')
  for (const [name, value] of [
    ['margin', '0'],
    ['color', 'var(--muted)'],
    ['font-size', 'var(--fs-md)'],
  ] as [string, string][])
    expect(prop(rule.body, name), `.tile-engine: ${name}`).toBe(value)
})

// ── H6a.8: the badge ───────────────────────────────────────────────────────

test('H6a.8: .badge is the pill of one fill, its text and the shape, and the shape rules are the ones of today', () => {
  const rule = ruleFor('.badge', 'the state badge')
  for (const [name, value] of [
    ['display', 'inline-flex'],
    ['align-items', 'center'],
    ['min-height', '28px'],
    ['padding', '0 var(--space-3)'],
    ['border-radius', 'var(--radius-pill)'],
    ['background', 'var(--surface)'],
    ['border', '1px solid transparent'],
    ['font-size', 'var(--fs-sm)'],
    ['font-weight', '600'],
    ['color', 'var(--text)'],
  ] as [string, string][])
    expect(prop(rule.body, name), `.badge: ${name}`).toBe(value)
  // one fill for every state, so the shape and the text carry it (1.4.11)
  expect(prop(rule.body, 'background'), '.badge fills with --surface').toBe(
    'var(--surface)',
  )

  // The `.shape` rules are unchanged by H6a: the same declarations, in the
  // same rules. A rule that gains a name (`tile-shape`) is red here.
  for (const [sel, decls] of [
    [
      '.shape',
      [
        ['display', 'inline-block'],
        ['position', 'relative'],
        ['width', '0.75rem'],
        ['height', '0.75rem'],
        ['margin-right', 'var(--space-2)'],
        ['vertical-align', '-0.05em'],
        ['border-radius', '50%'],
      ],
    ],
    ['.shape-running', [['background', 'var(--success)']]],
    ['.shape-stopped', [['border', '2px solid var(--muted)']]],
    [
      '.shape-changing',
      [
        ['border', '2px solid var(--warning)'],
        [
          'background',
          'linear-gradient(90deg, var(--warning) 50%, transparent 50%)',
        ],
      ],
    ],
    ['.shape-failed', [['border-radius', '0']]],
    ['.shape-failed::after', [['transform', 'rotate(-45deg)']]],
  ] as [string, [string, string][]][]) {
    const shape = ruleFor(sel, 'the shape rule H6a leaves as it is')
    for (const [name, value] of decls)
      expect(prop(shape.body, name), `${sel}: ${name}`).toBe(value)
  }
})

// ── H6a.10 (follow-up): the shapes under forced colours ────────────────────

// [open, end] of every @media (forced-colors: active) block, in file order.
const forcedRanges = (): Array<[number, number]> => {
  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf('@media (forced-colors: active)', from)
    if (at < 0) return out
    const end = rangeEnd(at)
    out.push([at, end])
    from = end + 1
  }
}

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

// The rules of a forced-colours block whose selector list is exactly this.
const forced = (sel: string): Rule[] =>
  forcedRanges().flatMap(([open, end]) =>
    RULES.filter(
      (r) =>
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact === sel.replace(/ /g, ''),
    ),
  )

// The rules of every forced-colours block.
const inForced = (): Rule[] =>
  forcedRanges().flatMap(([open, end]) =>
    RULES.filter((r) => r.depth > 0 && r.at > open && r.at < end),
  )

// The block, as the brief writes it: five rules, each with its one colour
// declaration, in system colours.
const FORCED: Array<[string, string, string]> = [
  ['.shape', 'forced-color-adjust', 'none'],
  ['.shape-running', 'background', 'CanvasText'],
  ['.shape-stopped, .shape-changing', 'border-color', 'CanvasText'],
  [
    '.shape-changing',
    'background',
    'linear-gradient(90deg, CanvasText 50%, transparent 50%)',
  ],
  ['.shape-failed::before, .shape-failed::after', 'background', 'CanvasText'],
]

for (const [sel, name, want] of FORCED)
  test(`H6a.10: ${sel} declares ${name} as ${want}, in the block`, () => {
    const rules = forced(sel)
    expect(
      rules.map((r) => r.sel.replace(/\s+/g, ' ')),
      `${sel}: the shape has exactly one rule in the forced-colours blocks`,
    ).toHaveLength(1)
    expect(
      rules[0].depth,
      `${sel}: the block sits at the top level: no width media wraps the shapes, and they are not nested in another block`,
    ).toBe(1)
    expect(prop(rules[0].body, name), `${sel}: ${name}`).toBe(want)
    expect(
      declared(rules[0].body).filter((n) => n === name),
      `${sel}: ${name} is declared once`,
    ).toHaveLength(1)
    expect(
      prop(rules[0].body, name).includes('var(--'),
      `${sel}: ${name} is a system colour, not a token of the sheet: the user's forced palette keeps the shape (CSS Color Adjustment 1, 3.2)`,
    ).toBe(false)
  })

test('H6a.10: the block colours the five shapes and nothing else, and draws no geometry of its own', () => {
  const shapes = inForced().filter((r) => r.compact.includes('.shape'))
  expect(
    shapes.map((r) => r.sel.replace(/\s+/g, ' ')).sort(),
    'these five rules and no other name a shape inside a forced-colours block: every shape is covered, and no other cue joins here',
  ).toEqual(FORCED.map(([sel]) => sel).sort())
  for (const [sel, decls] of [
    ['.shape', ['forced-color-adjust']],
    ['.shape-running', ['background']],
    ['.shape-stopped, .shape-changing', ['border-color']],
    ['.shape-changing', ['background']],
    ['.shape-failed::before, .shape-failed::after', ['background']],
  ] as [string, string[]][])
    expect(
      declared(forced(sel)[0].body),
      `${sel}: the block overrides the colour and draws nothing: no size, radius, transform, border width or animation joins it (1.4.11, H1a.11)`,
    ).toEqual(decls)
})

test('H6a.10: the shapes keep their geometry where it is: no rule moved into the block, none left the sheet', () => {
  for (const sel of [
    '.shape',
    '.shape-running',
    '.shape-stopped',
    '.shape-changing',
    '.shape-failed',
    '.shape-failed::after',
  ])
    expect(
      RULES.filter((r) => r.compact === sel.replace(/ /g, '')).map(
        (r) => r.depth,
      ),
      `${sel}: the geometry rule of H6a.8 is still at the top level`,
    ).toContain(0)
  expect(
    CSS,
    'the block is the one the brief writes: no second at-rule, no width media in it',
  ).not.toMatch(/@media[^{]*forced-colors[^{]*@media/)
})
