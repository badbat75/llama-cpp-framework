// H4, the light/dark theme switch: the stylesheet side (fe/briefs/07-hearth-h4-theme.md
// section 1 and criteria H4.10 and H4.11) and the text pin of H4.4. jsdom does no
// layout and no cascade, so the sheet is read as text, the way HearthTokens.test.ts
// and DisabledLook.test.ts do; that file exports no helper, so the reader is a small
// local copy.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const MAIN = readFileSync(join(import.meta.dirname, 'main.tsx'), 'utf8')

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

// The `--name: value;` pairs of a block, in the order they are written.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

// Every declaration of a block, `name: value`, in order: a count that matches
// the custom properties pins that the block holds nothing else.
const declarations = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.trim())
    .filter((d) => d.includes(':'))
    .map((d) => d.replace(/\s+/g, ' '))

// The rule of a list, and only it; a miss names what was searched for.
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

const topBy = (compact: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === compact),
    what,
  )

const rangeEnd = (open: number): number => {
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

/** Every top-level at-rule block whose prelude contains `needle`. */
const atRanges = (needle: string): [number, number][] => {
  const found: [number, number][] = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(needle, from)
    if (at < 0) return found
    const end = rangeEnd(at)
    found.push([at, end])
    from = end + 1
  }
}

/** The rules inside the at-rule block that starts at `open`. */
const inside = (range: [number, number]): Rule[] =>
  RULES.filter((r) => r.depth > 0 && r.at > range[0] && r.at < range[1])

/** The rules of `ranges`, for a media block that appears more than once. */
const insideAll = (ranges: [number, number][]): Rule[] => ranges.flatMap(inside)

// The colour and shadow tokens of brief 06 H1a.2 (the rows of its table, in
// the order of the table), plus the three art tokens of brief 08 H7.5 (the
// drawing, declared after --shadow in every theme block): the dark scheme
// declares exactly these.
const DARK16: [string, string][] = [
  ['--page', '#1b1714'],
  ['--bg', '#231d19'],
  ['--surface', '#2b241f'],
  ['--field', '#1b1714'],
  ['--line', '#3a3029'],
  ['--text', '#f1e9df'],
  ['--muted', '#b5a698'],
  ['--control', '#8a7c6f'],
  ['--accent', '#dcaaf0'],
  ['--on-accent', '#1b1714'],
  ['--accent-soft', '#3a2a44'],
  ['--danger', '#ff9a8a'],
  ['--success', '#7ccf8e'],
  ['--warning', '#e8b45a'],
  ['--hatch', 'rgb(241 233 223 / 0.06)'],
  ['--shadow', '0 0 0 1px var(--line)'],
  ['--art-1', '#3a2a44'],
  ['--art-2', '#3b2f24'],
  ['--art-3', '#2b241f'],
]

// The selector of H4.10's block A, in the compact spelling this reader uses.
const SYSTEM_DARK = ":root:not([data-theme='light'])"

const systemDark = (): Rule =>
  single(
    insideAll(atRanges('@media (prefers-color-scheme: dark)')),
    'the one rule inside @media (prefers-color-scheme: dark)',
  )

// ── H4.10: the dark block ──────────────────────────────────────────────

test('H4.10: the dark media block holds exactly one rule, whose selector is :root:not([data-theme=light]) with no class', () => {
  const media = atRanges('@media (prefers-color-scheme: dark)')
  expect(media.length, 'one dark media block').toBe(1)
  expect(inside(media[0]!).length, 'exactly one rule in it').toBe(1)
  expect(
    systemDark().compact,
    'the selector of block A: the root, unless the page says light (no .dark class, so the OS decides before any script runs)',
  ).toBe(SYSTEM_DARK)
})

test('H4.10: block A declares exactly the 19 dark tokens (the 16 of H1a.2 and the three art tokens of brief 08 H7.5), in the order of the table, and nothing else', () => {
  const body = systemDark().body
  expect(customProps(body), 'the 19 names and values, in order').toEqual(DARK16)
  expect(
    declarations(body),
    '19 declarations: the block adds no other property',
  ).toHaveLength(DARK16.length)
})

test('H4.10: the top-level :root[data-theme=dark] repeats the same 19 tokens with the same values in the same order, plus color-scheme: dark, and nothing else', () => {
  const body = topBy(
    ":root[data-theme='dark']",
    "the top-level :root[data-theme='dark'] rule",
  ).body
  expect(customProps(body), 'block B: the same 19, in the same order').toEqual(
    DARK16,
  )
  expect(
    customProps(body),
    'block B agrees with block A, token for token, in the order written',
  ).toEqual(customProps(systemDark().body))
  expect(
    prop(body, 'color-scheme'),
    'native controls and scrollbars go dark with it',
  ).toBe('dark')
  expect(
    declarations(body),
    '20 declarations: the 19 tokens and color-scheme, nothing else',
  ).toHaveLength(DARK16.length + 1)
})

test('H4.10: the top-level :root[data-theme=light] declares only color-scheme: light, so a light choice on a dark OS keeps the native controls light', () => {
  const body = topBy(
    ":root[data-theme='light']",
    "the top-level :root[data-theme='light'] rule",
  ).body
  expect(
    prop(body, 'color-scheme'),
    'the light scheme for native controls',
  ).toBe('light')
  expect(
    declarations(body),
    'nothing else: the light tokens are already those of :root',
  ).toEqual(['color-scheme: light'])
})

test('H4.10: the top-level :root keeps color-scheme: light dark, so with no attribute the OS decides for the native parts too', () => {
  expect(
    prop(
      topBy(':root', 'the top-level :root token block').body,
      'color-scheme',
    ),
    'color-scheme: light dark stays on :root',
  ).toBe('light dark')
})

test('H4.10: the theme change adds no motion, so the switch paints at once (H1a.11 pins this for the whole sheet)', () => {
  const moving: string[] = []
  for (const r of RULES) {
    if (!r.compact.includes('.theme-switch')) continue
    for (const d of declarations(r.body)) {
      const name = d.slice(0, d.indexOf(':')).trim()
      if (/^(transition|animation)(-|$)/.test(name))
        moving.push(`${r.sel.replace(/\s+/g, ' ')}: ${name}`)
    }
  }
  expect(moving, 'no transition and no animation on the switch').toEqual([])
})

// ── H4.11: the pressed look ────────────────────────────────────────────

const PRESSED = ".theme-switch[aria-pressed='true']"
const PRESSED_RE = /(^|,)\.theme-switch\[aria-pressed='true'\](,|$)/

test('H4.11: the on state is the mockup pressed toggle: the soft accent fill, the accent text and border, after the pressed rule of the toolbar buttons', () => {
  const pressed = topBy(
    PRESSED,
    'the top-level .theme-switch[aria-pressed=true] rule',
  )
  expect(prop(pressed.body, 'background'), 'the fill is --accent-soft').toBe(
    'var(--accent-soft)',
  )
  expect(prop(pressed.body, 'color'), 'the text is --accent').toBe(
    'var(--accent)',
  )
  expect(
    prop(pressed.body, 'border-color'),
    'the border is --accent, the ring of the pressed state',
  ).toBe('var(--accent)')
  const secondary = topBy(
    ".secondary[aria-pressed='true']",
    "the top-level .secondary[aria-pressed='true'] rule",
  )
  expect(
    pressed.at > secondary.at,
    'written after the .secondary pressed rule, so a switch that also carries .secondary takes the calmer look',
  ).toBe(true)
})

test('H4.11: in forced-colours mode the pressed switch stays visible: no colour adjust, the Highlight fill, the HighlightText label and the Highlight border', () => {
  const blocks = atRanges('@media (forced-colors: active)')
  expect(
    blocks.length,
    'the forced-colours blocks of H1a, H2a and H6a.10: the new rule joins one of them, it opens no sixth',
  ).toBe(5)
  const rules = insideAll(blocks).filter((r) => PRESSED_RE.test(r.compact))
  expect(
    rules.length,
    'exactly one forced-colours rule for the pressed switch',
  ).toBe(1)
  const body = rules[0]!.body
  expect(
    prop(body, 'forced-color-adjust'),
    'the author colours of the pressed state survive the forced palette',
  ).toBe('none')
  expect(prop(body, 'background'), 'the system highlight is the fill').toBe(
    'Highlight',
  )
  expect(prop(body, 'color'), 'and the system text colour the label').toBe(
    'HighlightText',
  )
  expect(prop(body, 'border-color'), 'with the highlight border').toBe(
    'Highlight',
  )
})

test('H4.11: the icon has a fixed size and never shrinks, so pressing moves no text', () => {
  const icon = topBy('.theme-switchsvg', 'the .theme-switch svg rule')
  expect(prop(icon.body, 'width'), 'the icon box is fixed').toBe('1.25rem')
  expect(prop(icon.body, 'height'), 'and square').toBe('1.25rem')
  expect(
    prop(icon.body, 'flex'),
    'and takes no share of the free space, so the label does not move',
  ).toBe('none')
})

test('H4.11: the switch lays out icon and text on one line, at the small gap of the toolbars', () => {
  const base = topBy('.theme-switch', 'the top-level .theme-switch rule')
  expect(prop(base.body, 'display'), 'one line').toBe('inline-flex')
  expect(prop(base.body, 'align-items'), 'centred').toBe('center')
  expect(prop(base.body, 'gap'), 'the small space token').toBe('var(--space-2)')
})

// ── H4.4: read before the first paint, in main.tsx ─────────────────────

test('H4.4: main.tsx imports the theme module and calls its start function before createRoot', () => {
  // `import { start } from './theme.ts'` or `import start from './theme.ts'`:
  // the brief suggests the path in section 5 and pins the call's place.
  const named = /import\s*\{([^}]*)\}\s*from\s*'\.\/[^']*theme[^']*'/.exec(MAIN)
  const first = (named?.[1] ?? '').split(',')[0]!.trim()
  const namedBinding = first.includes(' as ')
    ? first.split(/\s+as\s+/).pop()!
    : first
  const def = /import\s+([A-Za-z0-9_$]+)\s+from\s*'\.\/[^']*theme[^']*'/.exec(
    MAIN,
  )
  const binding = (namedBinding || def?.[1] || '').trim()
  expect(
    binding,
    'main.tsx imports the start function of the theme module (a path naming the theme)',
  ).not.toBe('')
  const root = MAIN.indexOf('createRoot(')
  expect(root, 'main.tsx still creates the root').toBeGreaterThan(-1)
  const call = MAIN.indexOf(`${binding}(`)
  expect(call, `${binding}() is called in main.tsx`).toBeGreaterThan(-1)
  expect(
    call < root,
    `the stored choice is applied before createRoot(...).render(...), so the first paint carries it`,
  ).toBe(true)
})
