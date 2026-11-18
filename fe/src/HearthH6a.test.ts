// Hearth H6a, the CSS (fe/briefs/08-hearth-h6-overview.md, H6a.5 to H6a.8):
// the stylesheet text of the tile, pinned as the declarations of the named
// rules, the way HearthH2b.test.ts reads index.css (jsdom does no layout, and
// importing a test file would register its tests, so the reader is a local
// copy: comments dropped, the same rule regex, the same nesting depth, prop()
// the last value of a property in a block).
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
