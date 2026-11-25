// The Hearth tokens of fe/briefs/06-hearth-h1-tokens.md, H1a: the token table,
// the contrast pairs, the surface map, the focus rings and the "must not
// change" guards, pinned as stylesheet text (jsdom does no layout). The file
// reads index.css the way DisabledLook.test.ts does (comments dropped, the same
// rule regex and depth); that file exports no helper, so this parser is a
// small local copy of its reader.
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

// Top-level rules only (as in DisabledLook.test.ts).
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

// The `--name: value;` pairs of a block, in order: the count pins duplicates.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

// Like DisabledLook's `one`, but usable from module scope and from helpers,
// where vitest's expect has no test context: a throw names the miss.
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

// The end of the at-rule block that starts at this offset, by its matched
// closing brace, the way the 7rem test locates @media (max-height: 31.98rem).
const rangeEnd = (open: number): number => {
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

const atRange = (needle: string): [number, number] => {
  const open = CSS.indexOf(needle)
  if (open < 0) throw new Error(`no ${needle} block in index.css`)
  return [open, rangeEnd(open)]
}

// The light token block: the top-level rule whose selector is exactly :root.
// The dark one: the rule whose selector is
// `:root:not([data-theme='light'])` inside the prefers-color-scheme: dark
// media block (brief 07 H4.10).
const lightBody = (): string =>
  single(
    TOP.filter((r) => r.compact === ':root'),
    'the top-level :root token block',
  ).body
const darkBody = (): string => {
  const [open, end] = atRange('@media (prefers-color-scheme: dark)')
  return single(
    RULES.filter(
      (r) =>
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact === ":root:not([data-theme='light'])",
    ),
    "the :root:not([data-theme='light']) rule inside the @media (prefers-color-scheme: dark) block",
  ).body
}
const lightTokens = (): [string, string][] => customProps(lightBody())
const darkTokens = (): [string, string][] => customProps(darkBody())

// The value of one token in a parsed block; a miss throws naming the token,
// so a contrast pair that lacks a token goes red on the token, not on NaN.
const valueOf = (pairs: [string, string][], name: string): string => {
  const hits = pairs.filter((p) => p[0] === name).map((p) => p[1])
  if (hits.length !== 1)
    throw new Error(`${name} is declared ${hits.length} times in the block`)
  return hits[0]
}

// WCAG 2.2 relative luminance and contrast ratio (brief section 2, the
// 0.04045 threshold), computed from the hex values READ from the stylesheet.
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
  return (ls[0] + 0.05) / (ls[1] + 0.05)
}

// The table of brief section 1, copied: [token, light value, dark value].
// A null dark value: the token is not redeclared in the dark block. The
// values are compared as written (hex lower case, rgb() space syntax).
const TOKENS: [string, string, string | null][] = [
  ['--page', '#f7f1e8', '#1b1714'],
  ['--bg', '#fffdf9', '#231d19'],
  ['--surface', '#f5efe6', '#2b241f'],
  ['--field', '#fffdf9', '#1b1714'],
  ['--line', '#e6dccf', '#3a3029'],
  ['--text', '#2a211c', '#f1e9df'],
  ['--muted', '#6b5d52', '#b5a698'],
  ['--control', '#8c7d70', '#8a7c6f'],
  ['--accent', '#7b3a8f', '#dcaaf0'],
  ['--on-accent', '#ffffff', '#1b1714'],
  ['--accent-soft', '#f1e4f6', '#3a2a44'],
  ['--danger', '#b3261e', '#ff9a8a'],
  ['--success', '#2f6f3e', '#7ccf8e'],
  ['--warning', '#8a5300', '#e8b45a'],
  ['--hatch', 'rgb(42 33 28 / 0.07)', 'rgb(241 233 223 / 0.06)'],
  [
    '--shadow',
    '0 1px 2px rgb(60 40 20 / 0.08), 0 6px 20px rgb(60 40 20 / 0.06)',
    '0 0 0 1px var(--line)',
  ],
  // Brief 08 H7.5: the three drawing tokens, in all three theme blocks.
  // Decoration, so they join no contrast pair (the pairs table below).
  ['--art-1', '#ead9f0', '#3a2a44'],
  ['--art-2', '#f3e3cf', '#3b2f24'],
  ['--art-3', '#ffffff', '#2b241f'],
  ['--space-1', '4px', null],
  ['--space-2', '8px', null],
  ['--space-3', '12px', null],
  ['--space-4', '16px', null],
  ['--space-5', '24px', null],
  ['--space-6', '40px', null],
  ['--space-7', '64px', null],
  ['--radius', '10px', null],
  ['--radius-check', '5px', null],
  ['--radius-card', '16px', null],
  ['--radius-pill', '999px', null],
  // The stacks H1b gives them: the face, its metric fallback, then today's.
  [
    '--font',
    "'Atkinson Hyperlegible Next', 'Atkinson Hyperlegible Next Fallback', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif",
    null,
  ],
  [
    '--mono',
    "'Atkinson Hyperlegible Mono', 'Atkinson Hyperlegible Mono Fallback', ui-monospace, 'Cascadia Mono', Menlo, monospace",
    null,
  ],
  ['--fs-xs', '0.8125rem', null],
  ['--fs-sm', '0.875rem', null],
  ['--fs-md', '0.9375rem', null],
  ['--fs-body', '1.0625rem', null],
  ['--fs-lg', '1.125rem', null],
  ['--fs-h2', '1.375rem', null],
  ['--fs-h1', '2rem', null],
  ['--lh', '1.55', null],
  ['--control-h', '40px', null],
  ['--border-w', '1.5px', null],
  ['--ring-w', '3px', null],
]

// The tokens of the table with a dark value (16 of brief 06, plus the three
// art tokens of brief 08 H7.5): the dark block declares exactly these.
const DARK_NAMES = TOKENS.filter((t) => t[2] !== null).map((t) => t[0])

// The --fs- tokens of the table (rows 30 to 36).
const FS = TOKENS.map((t) => t[0]).filter((n) => n.startsWith('--fs-'))

// The pairs of brief section 6, copied as given: [fg, bg, floor], each in
// both themes.
type Pair = [string, string, number]
const PAIRS: Pair[] = [
  ['--text', '--page', 4.5],
  ['--text', '--bg', 4.5],
  ['--text', '--surface', 4.5],
  ['--text', '--field', 4.5],
  ['--text', '--accent-soft', 4.5],
  ['--muted', '--page', 4.5],
  ['--muted', '--bg', 4.5],
  ['--muted', '--surface', 4.5],
  ['--muted', '--field', 4.5],
  ['--muted', '--accent-soft', 4.5],
  ['--accent', '--page', 4.5],
  ['--accent', '--bg', 4.5],
  ['--accent', '--surface', 4.5],
  ['--accent', '--field', 4.5],
  ['--accent', '--accent-soft', 4.5],
  ['--danger', '--page', 4.5],
  ['--danger', '--bg', 4.5],
  ['--danger', '--surface', 4.5],
  ['--danger', '--field', 4.5],
  ['--danger', '--accent-soft', 4.5],
  ['--success', '--page', 4.5],
  ['--success', '--bg', 4.5],
  ['--success', '--surface', 4.5],
  ['--success', '--field', 4.5],
  ['--success', '--accent-soft', 4.5],
  ['--warning', '--page', 4.5],
  ['--warning', '--bg', 4.5],
  ['--warning', '--surface', 4.5],
  ['--warning', '--field', 4.5],
  ['--warning', '--accent-soft', 4.5],
  ['--on-accent', '--accent', 4.5],
  ['--control', '--page', 3],
  ['--control', '--bg', 3],
  ['--control', '--surface', 3],
  ['--control', '--field', 3],
  ['--control', '--accent-soft', 3],
]
const THEMES = ['light', 'dark'] as const

test('H1a.1: the top-level :root declares exactly the 43 Hearth token names, none twice, and keeps color-scheme', () => {
  const pairs = lightTokens()
  const names = pairs.map((p) => p[0])
  expect(
    names.length,
    `${names.length} custom properties: a repeat counted`,
  ).toBe(TOKENS.length)
  expect(
    [...names].sort(),
    'exactly the 43 names of the table (40 of brief 06 plus the three art tokens of brief 08 H7.5), nothing extra',
  ).toEqual([...TOKENS.map((t) => t[0])].sort())
  expect(
    prop(lightBody(), 'color-scheme'),
    'color-scheme: light dark stays on :root',
  ).toBe('light dark')
})

test.each(TOKENS.map((t) => [t[0], t[1]] as [string, string]))(
  'H1a.1: %s has the light value of the table',
  (name, light) => {
    expect(
      valueOf(lightTokens(), name),
      `${name} in the top-level :root, as written in the table`,
    ).toBe(light)
  },
)

test('H1a.2: the dark :root declares exactly the 19 colour and shadow tokens, none twice, and nothing else', () => {
  const pairs = darkTokens()
  const names = pairs.map((p) => p[0])
  expect(
    names.length,
    `${names.length} custom properties in the dark block: a repeat counted`,
  ).toBe(DARK_NAMES.length)
  expect(
    [...names].sort(),
    'tokens 1 to 19 of the table, the colour and shadow set, nothing else',
  ).toEqual([...DARK_NAMES].sort())
})

test.each(
  TOKENS.filter((t) => t[2] !== null).map(
    (t) => [t[0], t[2] as string] as [string, string],
  ),
)('H1a.2: %s has the dark value of the table', (name, dark) => {
  expect(
    valueOf(darkTokens(), name),
    `${name} in the dark :root, as written in the table`,
  ).toBe(dark)
})

test('the ratio function: black on white is 21, and the brief-stated 14.03 and 4.96 of the parsed light tokens', () => {
  expect(contrast('#000000', '#ffffff').toFixed(2), 'black on white').toBe(
    '21.00',
  )
  expect(
    contrast('#ffffff', '#000000').toFixed(2),
    'the order of the pair does not matter',
  ).toBe('21.00')
  const p1 = contrast(
    valueOf(lightTokens(), '--text'),
    valueOf(lightTokens(), '--page'),
  )
  expect(
    p1.toFixed(2),
    `--text on --page light: measured ${p1.toFixed(2)}:1, the brief states 14.03:1`,
  ).toBe('14.03')
  const p25 = contrast(
    valueOf(lightTokens(), '--success'),
    valueOf(lightTokens(), '--accent-soft'),
  )
  expect(
    p25.toFixed(2),
    `--success on --accent-soft light, the lowest text pair: measured ${p25.toFixed(2)}:1, the brief states 4.96:1`,
  ).toBe('4.96')
})

test('the pairs table is the brief section 2: 31 text pairs at 4.5, 5 non-text pairs at 3', () => {
  expect(PAIRS.filter((p) => p[2] === 4.5).length, 'the P rows').toBe(31)
  expect(PAIRS.filter((p) => p[2] === 3).length, 'the N rows').toBe(5)
  expect(PAIRS.length, '36 pairs, each checked in both themes').toBe(36)
})

test.each(
  PAIRS.flatMap((p) =>
    THEMES.map(
      (t) => [p[0], p[1], p[2], t] as [string, string, number, string],
    ),
  ),
)('H1a.3: %s on %s, floor %s:1 in %s', (fg, bg, floor, theme) => {
  const pairs = theme === 'light' ? lightTokens() : darkTokens()
  const ratio = contrast(valueOf(pairs, fg), valueOf(pairs, bg))
  expect(
    ratio,
    `${fg} on ${bg} in ${theme}: ${ratio.toFixed(2)}:1, floor ${floor}:1`,
  ).toBeGreaterThanOrEqual(floor)
})

test('H1a.4: no colour literal outside the two token blocks and .dialog::backdrop', () => {
  const blocks = new Set([
    single(
      TOP.filter((r) => r.compact === ':root'),
      'the top-level :root token block',
    ).at,
    single(
      (() => {
        const [open, end] = atRange('@media (prefers-color-scheme: dark)')
        return RULES.filter(
          (r) =>
            r.depth > 0 &&
            r.at > open &&
            r.at < end &&
            r.compact === ":root:not([data-theme='light'])",
        )
      })(),
      "the :root:not([data-theme='light']) rule inside the @media (prefers-color-scheme: dark) block",
    ).at,
    // The dark tokens repeated for a stored choice (brief 07 H4.10) hold the
    // same hex values, so they are exempt too.
    ...TOP.filter((r) => r.compact === ":root[data-theme='dark']").map(
      (r) => r.at,
    ),
  ])
  // A # then 3, 4, 6 or 8 hex digits, and the colour functions, each with a
  // boundary so border-color( and a -1 do not match. Selectors are not
  // searched: an id selector is not a colour. Keywords stay allowed.
  const HEX =
    /(?<![0-9a-zA-Z-])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})(?![0-9a-zA-Z-])/g
  const FN =
    /(^|[^-a-zA-Z0-9])(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)\(/g
  const offenders: string[] = []
  for (const r of RULES) {
    if (blocks.has(r.at) || r.compact === '.dialog::backdrop') continue
    for (const re of [HEX, FN])
      for (const m of r.body.matchAll(re))
        offenders.push(`${r.sel.replace(/\s+/g, ' ')}: ${m[0].trim()}`)
  }
  expect(
    offenders,
    'every colour a rule draws is a var(--token); the only literal outside the token blocks is the .dialog::backdrop',
  ).toEqual([])
})

test('H1a.5: the body rule sits on --page with --text, --font, --fs-body and --lh', () => {
  const body = single(
    TOP.filter((r) => r.compact === 'body'),
    'the top-level body rule',
  ).body
  expect(
    prop(body, 'background'),
    'the page is the warm paper, cards lift off it',
  ).toBe('var(--page)')
  expect(prop(body, 'color'), 'body text is --text').toBe('var(--text)')
  expect(prop(body, 'font-family'), 'the font stack is a token').toBe(
    'var(--font)',
  )
  expect(prop(body, 'font-size'), '17 px from --fs-body').toBe('var(--fs-body)')
  expect(prop(body, 'line-height'), 'the leading is --lh').toBe('var(--lh)')
})

test('H1a.6: the headings take the type-scale tokens, and h1 keeps its 1.2 leading, its margins and its overflow-wrap', () => {
  const h1 = single(
    TOP.filter((r) => r.compact === 'h1'),
    'the top-level h1 rule',
  ).body
  expect(prop(h1, 'font-size'), 'h1 grows to --fs-h1').toBe('var(--fs-h1)')
  expect(prop(h1, 'line-height'), 'and a tight 1.2 leading').toBe('1.2')
  expect(prop(h1, 'margin'), 'margins unchanged').toBe('0 0 var(--space-4)')
  expect(
    prop(h1, 'overflow-wrap'),
    'the model profile id breaks anywhere',
  ).toBe('anywhere')
  const size = (sel: string, want: string, what: string): void => {
    const body = single(
      TOP.filter((r) => r.compact === sel),
      `the top-level ${sel} rule`,
    ).body
    expect(prop(body, 'font-size'), what).toBe(want)
  }
  size('h2', 'var(--fs-h2)', 'h2 is --fs-h2')
  size('h3', 'var(--fs-lg)', 'the card titles are --fs-lg')
  size('h4', 'var(--fs-body)', 'h4 is the body size')
})

test('H1a.7: every font-size outside :root is a var(--fs-*) of the scale, no rem, px or em literal', () => {
  const literals: string[] = []
  for (const r of RULES) {
    if (r.compact === ':root') continue
    for (const d of r.body.split(';')) {
      const i = d.indexOf(':')
      if (i < 0 || d.slice(0, i).trim() !== 'font-size') continue
      const val = d.slice(i + 1).trim()
      // H6a.7 writes the id's size as the literal 1.25rem (08-hearth-h6):
      // sanctioned for that rule, with that value, nowhere else.
      if (r.compact === '.tile-id' && val === '1.25rem') continue
      const m = /^var\((--fs-[a-z0-9-]+)\)$/.exec(val)
      if (!m || !FS.includes(m[1]))
        literals.push(`${r.sel.replace(/\s+/g, ' ')}: font-size: ${val}`)
    }
  }
  expect(
    literals,
    'each font-size names one of the --fs- tokens of the table',
  ).toEqual([])
})

test('H1a.8: cards and the save bar sit on --bg, enabled fields and the unchecked box on --field, rows divide on --line, the open About on --accent-soft, the nav and the banner sit on the paper (H5b.1, H5a.2)', () => {
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === '.card'),
        'the top-level .card rule',
      ).body,
      'background',
    ),
    'a card is the lifted --bg',
  ).toBe('var(--bg)')
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === '.save-bar'),
        'the top-level .save-bar rule',
      ).body,
      'background',
    ),
    'the save bar is --bg',
  ).toBe('var(--bg)')
  const enabled = single(
    TOP.filter((r) =>
      ["input[type='text']", "input[type='password']", 'select'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    "the enabled field rule (input[type='text'], input[type='password'], select)",
  ).body
  expect(
    prop(enabled, 'background'),
    'an enabled field is filled with --field',
  ).toBe('var(--field)')
  expect(
    prop(enabled, 'border'),
    'the field border is the --border-w one of H2a, in --control',
  ).toBe('var(--border-w) solid var(--control)')
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === '.model-profiles-filter-rowinput'),
        'the .model-profiles-filter-row input rule',
      ).body,
      'background',
    ),
    'the filter input is filled with --field',
  ).toBe('var(--field)')
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === "input[type='checkbox']"),
        "the top-level input[type='checkbox'] rule",
      ).body,
      'background',
    ),
    'the unchecked box is filled with --field',
  ).toBe('var(--field)')
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === 'td'),
        'the top-level td rule',
      ).body,
      'border-bottom',
    ),
    'table rows divide on the decorative --line',
  ).toBe('1px solid var(--line)')
  expect(
    prop(
      single(
        TOP.filter((r) => r.compact === ".about[aria-expanded='true']"),
        "the .about[aria-expanded='true'] rule",
      ).body,
      'background',
    ),
    'the open About shows --accent-soft, visible on the --bg card',
  ).toBe('var(--accent-soft)')
  // H5a.2 (07-hearth-h5-shell.md) restyled the banner: transparent, no
  // divider. H5b.1 gave the nav the same paper.
  const banner = single(
    TOP.filter((r) => r.compact === '.banner'),
    'the top-level .banner rule',
  ).body
  expect(
    prop(banner, 'background'),
    'the banner is transparent: the shell sits on the page (H5a.2)',
  ).toBe('transparent')
  expect(
    ['', '0'].includes(prop(banner, 'border-bottom')),
    'the banner has no border-bottom declaration, or border-bottom: 0 (H5a.2)',
  ).toBe(true)
  const nav = single(
    TOP.filter((r) => r.compact === '.nav'),
    'the top-level .nav rule',
  ).body
  expect(
    prop(nav, 'background'),
    'the nav is transparent: the shell sits on the page with the banner (H5b.1)',
  ).toBe('transparent')
  expect(
    prop(nav, 'border-bottom'),
    'the nav bottom edge is the decorative 1px solid --line of the open menu (H5b.1)',
  ).toBe('1px solid var(--line)')
})

test('H1a.11: no transition, transition-*, animation or animation-* declaration, so nothing moves and prefers-reduced-motion needs no rule', () => {
  const moving: string[] = []
  for (const r of RULES)
    for (const d of r.body.split(';')) {
      const i = d.indexOf(':')
      if (i < 0) continue
      const name = d.slice(0, i).trim()
      if (/^(transition|animation)(-|$)/.test(name))
        moving.push(`${r.sel.replace(/\s+/g, ' ')}: ${name}`)
    }
  expect(
    moving,
    'Hearth adds no motion: a transition here would need a reduced-motion rule',
  ).toEqual([])
  expect(
    CSS,
    'no prefers-reduced-motion block: nothing moves, none is needed',
  ).not.toMatch(/prefers-reduced-motion/)
})

test('H1a.12 guard: the focus rings keep today declarations, the accent ring whose contrast P11 to P15 pin', () => {
  const ring = single(
    TOP.filter((r) => r.compact === ':focus-visible'),
    'the top-level :focus-visible rule',
  ).body
  expect(
    prop(ring, 'outline'),
    'the ring is the --ring-w accent one of H2a.7, whose contrast P11 to P15 pin',
  ).toBe('var(--ring-w) solid var(--accent)')
  expect(prop(ring, 'outline-offset'), 'offset unchanged').toBe('2px')
  const moved = single(
    TOP.filter((r) => r.compact === "[tabindex='-1']:focus"),
    "the [tabindex='-1']:focus rule",
  ).body
  expect(prop(moved, 'outline'), 'a focus the page moves needs no ring').toBe(
    'none',
  )
  const heading = single(
    TOP.filter((r) => r.compact === '.cardh3:focus-visible'),
    'the .card h3:focus-visible rule',
  ).body
  expect(
    prop(heading, 'outline'),
    'the jump landing keeps its accent ring, at the same --ring-w width',
  ).toBe('var(--ring-w) solid var(--accent)')
  expect(prop(heading, 'outline-offset'), 'with its offset').toBe('2px')
})

test('H1a.12 guard: the four forced-colours blocks stay, with system colours only (the token colours are forced there)', () => {
  const SYSTEM = [
    'GrayText',
    'CanvasText',
    'Canvas',
    'Highlight',
    'HighlightText',
    // H9a.5 (brief 08-hearth-h9-jump): the borderless jump entry reads as the
    // link it looks like, so the forced palette gives it the link colour.
    'LinkText',
    // 21 T.23 (brief 21, TP1b): the x and the + of the Tensor placement list
    // keep their glyph under forced colours, drawn in the system button text.
    'ButtonText',
  ]
  const blocks: string[] = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf('@media (forced-colors: active)', from)
    if (at < 0) break
    const end = rangeEnd(at)
    blocks.push(CSS.slice(at, end + 1))
    from = end + 1
  }
  expect(
    blocks.length,
    'the exact count, not a floor: the three blocks of H1a (the row switch aria-disabled box, the aria-disabled buttons, the checkbox row box), the one H2a.6 added for the switch knob, and the one H6a.10 added for the state shapes, and no more: a new block says a new corner of the page needs forced colours, and a lost one that a cue went away',
  ).toBe(5)
  // `inherit` is a CSS-wide keyword, not an author colour: H3.10 gives the
  // .mark rule `color: inherit` in a forced block so the mark takes the home
  // link's forced colour (the UA sheet pins an svg's colour to the author
  // value, brief 07 section 0). Allowed as the WHOLE value, case-insensitive:
  // a value glued to anything else (inherit-ish) is not the keyword. Nothing
  // else joins the list: currentColor, unset, initial and a var() stay red.
  // H6a.10 adds the one gradient: the half fill of the changing shape, its two
  // stops the system text colour and nothing (the same pair the top-level rule
  // draws with the tokens).
  const GRADIENT = 'linear-gradient(90deg, canvastext 50%, transparent 50%)'
  const allowed = (v: string): boolean =>
    SYSTEM.includes(v) ||
    v.toLowerCase() === 'inherit' ||
    v.replace(/\s+/g, ' ').toLowerCase() === GRADIENT
  const off: string[] = []
  for (const b of blocks)
    for (const m of b.matchAll(/([-a-zA-Z]+)\s*:\s*([^;{}]+);/g)) {
      // forced-color-adjust is the one property that is not a colour: the
      // switch knob needs it (H2a.6), and naming it keeps the scan honest.
      // H7.7 (brief 08) removes the sign-in drawing in forced colours: a
      // display: none is not a colour either, so it joins the named pair,
      // property and value together, keeping the scan honest.
      if (m[1] === 'display' && m[2].trim() === 'none') continue
      if (m[1] !== 'forced-color-adjust' && !allowed(m[2].trim()))
        off.push(`${m[1]}: ${m[2].trim()}`)
    }
  expect(
    off,
    'inside a forced-colours block only the system colour keywords and the CSS-wide keyword inherit: a var(), a hex, a named colour or any other keyword is an author colour the forced palette fights; the cues that survive are the border styles, the shapes and the text',
  ).toEqual([])
})

test('H1a.12 guard: the sizes that moved are the controls, and H2a draws with the shape tokens H1a declared', () => {
  // Every min-height and min-width in the sheet, selector and value, in the
  // order they are written: a set of the VALUES would let a control take
  // another control's size (a 24 px row and a 40 px control are both in the
  // set), the pairs pin which rule carries which.
  const sizes: string[] = []
  for (const r of RULES)
    for (const n of ['min-height', 'min-width']) {
      const v = prop(r.body, n)
      if (v) sizes.push(`${r.sel.replace(/\s+/g, ' ')}: ${n}: ${v}`)
    }
  // Brief 19, 19 G.12 (fe/briefs/19-layer-sliders.md): the block slider of a
  // GPU row is at least 24 px high (2.5.8) and shrinks to the row, so it names
  // its own two sizes under a class that says blocks (the coder names the
  // class, this pins the name says it). The rest of the sheet is unchanged.
  const sliderSizes = sizes.filter((s) => /\bblocks\b/.test(s))
  expect(
    sliderSizes.map((s) => s.slice(s.indexOf(': ') + 2)),
    '19 G.12: the block slider carries min-height 24px and min-width 0 in the sheet, under a class whose name says blocks, and nothing else of its own',
  ).toEqual(['min-height: 24px', 'min-width: 0'])
  // Brief 21, its TP1 review of 2026-11-23 (fe/briefs/21-tensor-placement.md):
  // each visible label and its control of a rule row is one flex item of the
  // row, so each of the three items the review names (Tensor, Goes on,
  // Pattern) carries min-width 0 and no other size (21 section 6: no new
  // min-height or min-width beyond min-width: 0 on the row items).
  // Its D4 re-check of the same day: the select inside the Tensor and Goes on
  // items keeps its longest option's width, so the fix gives both selects
  // their own min-width: 0, the items keep theirs. Its D5 second re-check:
  // the Layers span gets min-width: 0 too, and so its input (the D4 cap
  // alone is measured against the span, whose min-content is the input's
  // 10ch). The pin is per name: the
  // coder writes the two selects as two rules or one comma group (the sheet's
  // style for a shared declaration, index.css 1399), and the names are
  // the same either way. Every size in the card stays min-width 0.
  const otSizes = sizes.filter((s) => /ot-|tensor|placement/.test(s))
  // Brief 21 TP1b (21 T.20, 21 T.21, 21 T.22, fe/briefs/21-tensor-placement.md):
  // the row's Remove and the Add rule under the list become icon buttons, each
  // a 40 px square under a class of the card's namespace (the coder names
  // them: `.ot-remove`, `.ot-add`). They are the only sizes in the card that
  // are not min-width: 0, and each names its own 40 px floor: a 24 px button
  // beside 40 px selects, or a size on a third class, is red here.
  const ICON = ['.ot-add', '.ot-remove']
  const iconSize = (s: string): string[] => {
    const cut = s.indexOf(': ')
    const parts = s
      .slice(0, cut)
      .split(',')
      .map((p) => p.trim())
    return parts.every((p) => ICON.includes(p))
      ? parts.map((p) => `${p}${s.slice(cut)}`)
      : []
  }
  expect(
    otSizes.flatMap(iconSize).sort(),
    '21 T.20 and 21 T.21: the two icon buttons of the Tensor placement list carry min-height 40px and nothing else, each under its own class (a comma group counts for both)',
  ).toEqual(['.ot-add: min-height: 40px', '.ot-remove: min-height: 40px'])
  for (const s of otSizes.filter((s) => iconSize(s).length === 0))
    expect(
      s,
      '21 TP1 review D1, D4 and D5: every size in the Tensor placement card is min-width: 0: no min-height and no other value',
    ).toMatch(/: min-width: 0$/)
  expect(
    [
      ...new Set(
        otSizes
          .filter((s) => iconSize(s).length === 0)
          .flatMap((s) => s.slice(0, s.indexOf(': ')).split(','))
          .map((p) => p.trim()),
      ),
    ].sort(),
    '21 TP1 review D4 and D5: min-width: 0 is carried by the three row items, by the Layers span and its input, and by both selects inside the Tensor and Goes on items: the D1 fix put each select inside its item, where the auto minimum size keeps its longest option width and the page scrolls sideways at 320 px (build/ux-review/tp1/run2.txt); D5 is the same rule for the Layers span and input, measured 30 px past the column at 200 % text (run3.txt)',
  ).toEqual(
    [
      '.ot-tensor',
      '.ot-tensor select',
      '.ot-goes-on',
      '.ot-goes-on select',
      '.ot-pattern',
      '.ot-layers',
      '.ot-layers input.number',
    ].sort(),
  )
  const rest = sizes.filter(
    (s) => !/\bblocks\b/.test(s) && !/ot-|tensor|placement/.test(s),
  )
  expect(
    rest,
    'the sizes of the page, per rule: the controls are --control-h (buttons, fields, the filter input, the 720 px label, the icon-only theme switch below 480 px), the switch label, the checkbox row and the pill keep 24px, the jump list 32px until H9, the status lines 1lh, the app 100vh, and a flex child keeps min-width 0 so it can shrink; the id of the tile shrinks in its head row and the badge pill is 28 px (H6a.7, H6a.8), the save bar button row shrinks with the bar (H5c.7), the x of a chip keeps its 24 px and the + and the icons of the pill their 32 px circles (15 section 10 R2), and the chip text, the new-tag pill and its box shrink to the row (its R3), and the sortable header button keeps 24 px (17c.9), and the fact row of a tile and its value cell shrink below their content (17b), and the tag suggestion list shrinks to the field (15 section 11 D2), and the confirm group of a list row shrinks in its cell (22 D4.8)',
  ).toEqual([
    '#root: min-height: 100vh',
    'button: min-height: var(--control-h)',
    'button: min-width: 24px',
    // H5a.5: the icon-only theme switch below 480 px keeps the --control-h
    // square (2.5.8); H5b.2: the nav pills are 32 px (2.5.8 asks 24). The
    // Sign in pill of H5a.4 is gone (09 S1a.12).
    '.nav a: min-height: 32px',
    // Brief 20, 20 N.11: the text run of an instance link shrinks in the pill
    // (min-width 0), so a 40-character id wraps instead of scrolling the page
    // sideways at 320 px (1.4.10). Brief 20 says this list is unchanged "no
    // size", meaning no height: the rule it mandates carries a min-width, and
    // this pin is of every min-height AND min-width in the sheet.
    '.nav-instance: min-width: 0',
    'main: min-width: 0',
    '.theme-switch: min-width: var(--control-h)',
    // 17c.9: the sortable header's text is a button, so it names its own 24 px
    // floor (2.5.8); it has no class of its own.
    '.model-profiles th button: min-height: 24px',
    '.model-profiles-switch: min-height: 24px',
    '.model-profiles-switch: min-width: 24px',
    '.model-profiles-switch-status: min-height: 1lh',
    '.model-profiles-filter-row input: min-height: var(--control-h)',
    '.model-profiles-filter-row input: min-width: 0',
    '.model-profiles-filter-status: min-height: 1lh',
    // H6a.7: the id shrinks inside its tile; H6a.8: the pill is 28 px.
    '.tile-id: min-width: 0',
    // 17b.5 (brief 17 U17b): the fact row and its value cell are flex/grid
    // children, so min-width 0 lets them go below their content: the chips
    // list of the Tags cell wraps inside the tile instead of pushing the tile
    // past 320 px (the overflow the ux pass measured).
    '.tile dl > div: min-width: 0',
    '.tile dd: min-width: 0',
    '.badge: min-height: 28px',
    '.jump ul button: min-height: 32px',
    'fieldset: min-width: 0',
    '.field-control: min-width: 0',
    '.full-value: min-width: 0',
    '.field-label: min-height: var(--control-h)',
    // H11a.9 (brief 08-hearth-h10-h11): the Assets disclosure's touch-target
    // floor, a floor and not a box.
    '.assets-card h3 button: min-height: 32px',
    '.assets dt: min-width: 0',
    '.assets dd: min-width: 0',
    '.about: min-height: 24px',
    "input[type='text'], input[type='password'], select: min-height: var(--control-h)",
    "input[type='text'], input[type='password'], select: min-width: 0",
    '.check: min-height: 24px',
    // Brief 14, DESIGN.md 5.2 "GPU list" (GD1.12): the row's label is a flex
    // child, min-width 0 lets it shrink and break its 64-character id.
    '.check-list > li > .check: min-width: 0',
    // Brief 19, 19 G.13 (fe/briefs/19-layer-sliders.md): the layer slider of a
    // number field is an item of the wrapping control row, so it names its own
    // 24 px floor (2.5.8) and shrinks to the row beside the box and the input.
    // No class of its own: brief 10 R1.8 freezes the sheet's classes (the
    // brief's section 6 names no class for this slider), and a field row holds
    // one range input at most.
    ".field-control input[type='range']: min-height: 24px",
    ".field-control input[type='range']: min-width: 0",
    // Brief 22, 22 D4.8 (fe/briefs/22-model-sources.md): the confirm group of a
    // list row is in a table cell, so it shrinks below its content and a long
    // id wraps instead of widening the scroll box (1.4.10).
    '.confirm-group: min-width: 0',
    '.hint-box: min-width: 7rem',
    // H7.6 (brief 08): the sign-in panel is a grid child, min-width 0 lets it shrink.
    '.signin-panel: min-width: 0',
    '.save-messages: min-width: 0',
    // H5c.7: the row of the bar's buttons shrinks with the bar.
    '.save-actions: min-width: 0',
    // Brief 15 section 10 R2 and R3 (the tags as chips). R2: the global
    // `button { min-height: var(--control-h) }` makes the chip's x, the + and
    // the pill's icons 40 px high (the chip a 50 px pill, the + a 24 by 40
    // oval), so each control names its own size: the x the 24 px floor of
    // 2.5.8, the + and the pill's icons the 32 px circles of section 3. The
    // rule that names one is `.chip button` (its R1: the x has no class of its
    // own). `.chip` and `.tag-new` keep no min-height here: they grow from
    // their content, so a chip is as high as its x and the pill as high as its
    // box, which is what keeps the chip row at one chip per line at 320 px.
    // The order below is the order of the rules in the sheet, as it is for
    // every entry of this list.
    // R3: the chip's text, the pill and its box are flex children that shrink
    // below their content, so a 32-character tag and the open pill fit 320 px
    // and 200 % text (the ellipsis of the span, the 152 px of sideways scroll
    // the pill measured before its box shrank).
    // The row itself is a flex child of the group, so it shrinks below the
    // width of one chip (R3: the cap is the row, `.chip { max-width: 100% }`).
    '.chips: min-width: 0',
    '.chip > span: min-width: 0',
    '.chip button: min-height: 24px',
    // R5: the expanded pill covers its chip, so it is at least as tall as it;
    // its height comes from its text when it is several lines. Its section 10
    // R7: the rule is `.chip > .chip-full`, because the pill is a span child of
    // the chip and `.chip > span` (0,1,1) outranks a bare `.chip-full`.
    '.chip > .chip-full: min-height: 100%',
    '.tag-add: min-height: 32px',
    '.tag-add: min-width: 32px',
    '.tag-new: min-width: 0',
    // 2.5.8: the box of the open pill is a target like every text field.
    '.tag-new input: min-height: var(--control-h)',
    '.tag-new input: min-width: 0',
    '.tag-new button: min-height: 32px',
    '.tag-new button: min-width: 32px',
    // Brief 15 section 11, the C2 review (D2): the suggestions list is an
    // item of the wrapping flex .field-control, so min-width 0 lets it go
    // below its min-content and a 32-character label breaks inside 320 px at
    // 200 % text. Last: the rule sits after .tag-new button in the sheet.
    '.tag-options: min-width: 0',
  ])
  const used = new Set<string>()
  for (const r of RULES)
    if (r.compact !== ':root')
      for (const t of [
        '--control-h',
        '--border-w',
        '--ring-w',
        '--radius-check',
      ])
        if (r.body.includes(`var(${t})`)) used.add(t)
  expect(
    [...used].sort(),
    'H2a is the slice that draws with the shape tokens H1a declared: all four are in use',
  ).toEqual(['--border-w', '--control-h', '--radius-check', '--ring-w'])
})
