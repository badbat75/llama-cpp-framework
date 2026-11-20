// Hearth H7 (fe/briefs/08-hearth-h7-signin.md, section 1), the stylesheet part:
// H7.4 the colours of the shapes by class, H7.5 the three tokens in the three
// theme blocks, H7.6 the layout of the two columns and the lifted panel, H7.7
// the art hidden where it does not help. The stylesheet is read as text, the
// way HearthH2a.test.ts reads it: rules by selector, blocks by matched braces.
//
// What is pinned is the brief's text, so a value is compared as written, with
// its runs of whitespace collapsed (Prettier breaks a long declaration over
// lines). A rule that declares more than the brief lists is red: the panel's
// height and the shapes' colours are the two things the brief fixes in
// particular (1.4.12, and section 0's colours by class).
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

// Top-level rules only: a rule inside an @media block is an override, not the
// rule of the part.
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

// The rules whose selector carries this selector, inside a block of this
// at-rule.
const inside = (header: string, sel: string): Rule[] =>
  atRanges(header).flatMap(([open, end]) =>
    RULES.filter(
      (r) =>
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact.includes(sel.replace(/\s+/g, '')),
    ),
  )

// The class a selector part ends with: `.signin-art .art-sun` gives art-sun.
const lastClass = (part: string): string => {
  const m = /\.([a-zA-Z0-9_-]+)\s*$/.exec(part.trim())
  return m ? m[1]! : ''
}

// The top-level rules whose selector lists exactly these classes, in order.
const byClasses = (names: string[]): Rule[] =>
  TOP.filter((r) => {
    const got = r.sel.split(',').map(lastClass)
    return got.length === names.length && got.every((c, n) => c === names[n])
  })

// ── H7.4: the colours of the shapes, by class ─────────────────────────────

/** The ten rules of H7.4: the classes of the selector, and what it declares. */
const COLOUR_RULES: Array<{
  names: string[]
  decls: Array<[string, string]>
}> = [
  { names: ['art-sun', 'art-ground'], decls: [['fill', 'var(--art-1)']] },
  {
    names: ['art-door'],
    decls: [
      ['fill', 'var(--art-3)'],
      ['stroke', 'var(--text)'],
    ],
  },
  {
    names: ['art-panel'],
    decls: [
      ['fill', 'var(--bg)'],
      ['stroke', 'var(--text)'],
    ],
  },
  { names: ['art-bar-on'], decls: [['fill', 'var(--accent)']] },
  {
    names: ['art-bar'],
    decls: [
      ['fill', 'none'],
      ['stroke', 'var(--text)'],
    ],
  },
  { names: ['art-knob'], decls: [['fill', 'var(--on-accent)']] },
  { names: ['art-cable', 'art-stem'], decls: [['fill', 'none']] },
  { names: ['art-cable', 'art-line'], decls: [['stroke', 'var(--text)']] },
  { names: ['art-stem'], decls: [['stroke', 'var(--success)']] },
  { names: ['art-leaf'], decls: [['fill', 'var(--success)']] },
]

test('H7.4: every shape takes its colour by class from a token, in a rule scoped under .signin-art, and the rule adds nothing of its own', () => {
  for (const { names, decls } of COLOUR_RULES) {
    const what = `.signin-art ${names.join(', ')}`
    const rule = single(byClasses(names), `the colour rule of ${what}`)
    for (const [name, value] of decls)
      expect(norm(prop(rule.body, name)), `${what}: ${name}`).toBe(norm(value))
    expect(
      declared(rule.body),
      `${what}: the rule declares exactly ${decls.map(([n]) => n).join(', ')}`,
    ).toEqual(decls.map(([n]) => n))
    // H7.4 scopes the colours: the drawing sits in no other page, and a bare
    // .art-* rule would reach anything else that ever carries the class.
    for (const part of rule.sel.split(','))
      expect(
        part.trim().startsWith('.signin-art'),
        `${what}: the selector "${part.trim()}" is scoped under .signin-art`,
      ).toBe(true)
  }
})

// ── H7.5: the three tokens, in the three theme blocks ─────────────────────

/** The three theme blocks of section 0, with the theme each carries. */
const THEME_BLOCKS: Array<{
  what: string
  theme: 'light' | 'dark'
  body: string
}> = [
  {
    what: 'the light :root',
    theme: 'light',
    body: topBy(':root', 'the light tokens').body,
  },
  {
    what: 'the dark :root of the @media (prefers-color-scheme: dark) block',
    theme: 'dark',
    body: single(
      inside(
        '@media (prefers-color-scheme: dark)',
        ":root:not([data-theme='light'])",
      ),
      'the dark tokens of the system scheme',
    ).body,
  },
  {
    what: "the stored dark :root[data-theme='dark']",
    theme: 'dark',
    body: topBy(":root[data-theme='dark']", 'the stored dark tokens').body,
  },
]

/** The tokens of section 0's table: the name, the light value, the dark one. */
const ART_TOKENS: Array<[string, string, string]> = [
  ['--art-1', '#ead9f0', '#3a2a44'],
  ['--art-2', '#f3e3cf', '#3b2f24'],
  ['--art-3', '#ffffff', '#2b241f'],
]

const TOKEN_CASES = THEME_BLOCKS.flatMap((block) =>
  ART_TOKENS.map(([name, light, dark]) => [
    block.what,
    name,
    block.theme === 'light' ? light : dark,
    block.body,
  ]),
)

test.each(TOKEN_CASES)(
  'H7.5: %s declares %s with the value of the table',
  (what, name, value, body) => {
    expect(
      (body.match(new RegExp(`${name}\\s*:`, 'g')) ?? []).length,
      `${what}: ${name} is declared once`,
    ).toBe(1)
    expect(norm(prop(body, name)), `${what}: ${name}`).toBe(value)
  },
)

test('H7.5: the art tokens stay out of the contrast pairs (the drawing is decoration, so it answers to no floor)', () => {
  const src = readFileSync(
    join(import.meta.dirname, 'HearthTokens.test.ts'),
    'utf8',
  )
  const at = src.indexOf('const PAIRS')
  expect(at, 'the pairs table of the contrast tests').toBeGreaterThan(-1)
  const table = src.slice(at, src.indexOf('\n]', at))
  for (const [name] of ART_TOKENS)
    expect(table.includes(name), `${name} joins no pair`).toBe(false)
})

// ── H7.6: the two columns and the lifted panel ────────────────────────────

/** The four rules of H7.6, each with the declarations the brief lists. */
const LAYOUT: Array<{ sel: string; decls: Array<[string, string]> }> = [
  {
    sel: '.signin-page',
    decls: [
      ['display', 'grid'],
      [
        'grid-template-columns',
        'repeat(auto-fit, minmax(min(100%, 18rem), 1fr))',
      ],
      ['gap', 'var(--space-6)'],
      ['align-items', 'center'],
      ['margin-inline', 'auto'],
    ],
  },
  {
    sel: '.signin-art',
    decls: [
      ['background', 'var(--art-2)'],
      ['border-radius', '24px'],
      ['padding', 'var(--space-5)'],
      ['display', 'grid'],
      ['place-items', 'center'],
    ],
  },
  {
    sel: '.signin-art svg',
    decls: [
      ['width', '100%'],
      ['max-width', '420px'],
      ['height', 'auto'],
    ],
  },
  {
    sel: '.signin-panel',
    decls: [
      ['background', 'var(--bg)'],
      ['border', '1px solid transparent'],
      ['border-radius', 'var(--radius-card)'],
      ['box-shadow', 'var(--shadow)'],
      ['padding', 'var(--space-5)'],
      ['min-width', '0'],
    ],
  },
]

test('H7.6: the page is a two-column grid by room, the art a panel of its own, the form a lifted card, and none of them has a height of its own (1.4.12)', () => {
  for (const { sel, decls } of LAYOUT) {
    const rule = topBy(sel, `the layout of ${sel}`)
    for (const [name, value] of decls)
      expect(norm(prop(rule.body, name)), `${sel}: ${name}`).toBe(norm(value))
    expect(
      declared(rule.body),
      `${sel}: exactly ${decls.map(([n]) => n).join(', ')}: no height, no width, and no margin beside the auto inline margin of 10 R1.7`,
    ).toEqual(decls.map(([n]) => n))
  }
  // the form rule is unchanged: the panel carries the box, the form its width
  expect(
    norm(prop(topBy('.signin', 'the form').body, 'max-width')),
    '.signin keeps max-width: 24rem',
  ).toBe('24rem')
})

// ── H7.7: hidden where the drawing does not help ──────────────────────────

test('H7.7: the art is display: none below 480 px and in forced colours, where every fill becomes a system colour', () => {
  for (const header of [
    '@media (max-width: 479.98px)',
    '@media (forced-colors: active)',
  ]) {
    const hidden = inside(header, '.signin-art').filter(
      (r) =>
        lastClass(r.sel.split(',').pop() ?? '') === 'signin-art' &&
        norm(prop(r.body, 'display')) === 'none',
    )
    expect(hidden.length, `${header} holds .signin-art { display: none }`).toBe(
      1,
    )
  }
})
