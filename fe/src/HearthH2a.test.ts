// Hearth H2a, the controls (fe/briefs/06-hearth-h2-controls.md, section 1): a
// restyle, CSS only, so the tests pin the stylesheet text. jsdom does no
// layout, so this file reads index.css the way DisabledLook.test.ts and
// HearthTokens.test.ts do (comments dropped, the same rule regex, the same
// nesting depth, prop() the last value of a property in a block); neither file
// exports a helper, so the reader here is a small local copy of theirs.
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
  // combinator (".jump button"), which compact cannot.
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

// Like DisabledLook's `one`, usable from module scope and from helpers, where
// vitest's expect has no test context: a throw names the miss.
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
    `the top-level rule whose selector is exactly ${compact} (${what})`,
  )

const flatBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.flat === sel),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
  )

// The end of the at-rule block that starts at this offset, by its matched
// closing brace.
const rangeEnd = (open: number): number => {
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

// Every block written with this at-rule header, as [open, end] offsets: there
// is more than one @media (min-width: 720px), and H2a.6 adds a forced-colours
// block, so a locator takes all of them.
const atRanges = (needle: string): [number, number][] => {
  const ranges: [number, number][] = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(needle, from)
    if (at < 0) return ranges
    const end = rangeEnd(at)
    ranges.push([at, end])
    from = end + 1
  }
}

// The rules inside every block with this header whose selector holds `has`.
const inside = (needle: string, has: string): Rule[] => {
  const hits: Rule[] = []
  for (const [open, end] of atRanges(needle))
    for (const r of RULES)
      if (r.depth > 0 && r.at > open && r.at < end && r.compact.includes(has))
        hits.push(r)
  return hits
}

// The 1.5px dashed --control border, in the shorthand or split into a dashed
// style and the colour (both spellings, as 2h.163a and H2a.10 allow).
const dashedControl = (body: string): boolean =>
  prop(body, 'border') === 'var(--border-w) dashed var(--control)' ||
  (prop(body, 'border-style') === 'dashed' &&
    prop(body, 'border-color') === 'var(--control)')

// H2a.8's hatch, as the brief writes it.
const HATCH =
  'repeating-linear-gradient(135deg, transparent 0 6px, var(--hatch) 6px 8px)'

const CHECKBOX = "input[type='checkbox']"
const SWITCH = "input[role='switch']"

// ── Specificity, counted the simple way (H2a.6's forced-colours guard) ────
// A cascade answer is read off the stylesheet text here, so the test computes
// specificity the way the brief's argument does: (a) the attribute selectors,
// classes and pseudo-classes, (b) the elements and pseudo-elements. index.css
// has no id selector, so ids are not counted; a combinator is neither a state
// nor an element, so it goes in the signature (which elements a selector
// reaches) and never in (b).
type Sel = {
  raw: string
  states: string[]
  sig: string
  spec: [number, number]
}

const TOKEN =
  /\[[^\]]*\]|::[a-zA-Z-]+|:[a-zA-Z-]+(?:\([^)]*\))?|\.[a-zA-Z-]+|#[a-zA-Z-]+|[a-zA-Z][a-zA-Z0-9-]*|[*+>~]|\s+/g

const parseSel = (raw: string): Sel => {
  const states: string[] = []
  const sig: string[] = []
  let els = 0
  for (const m of raw.trim().matchAll(TOKEN)) {
    const t = m[0]
    if (/^\s/.test(t)) sig.push(' ')
    else if (t === '*' || t === '+' || t === '>' || t === '~') sig.push(t)
    else if (
      t[0] === '.' ||
      t[0] === '#' ||
      t[0] === '[' ||
      (t[0] === ':' && t[1] !== ':')
    )
      states.push(t)
    else {
      sig.push(t)
      els++
    }
  }
  return {
    raw: raw.trim(),
    states,
    sig: sig.join(''),
    spec: [states.length, els],
  }
}

// The items of a selector list, split on the commas outside brackets (an
// attribute value or a functional pseudo-class can hold one).
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

// A selector that reaches the switch's knob.
const isKnob = (raw: string): boolean => {
  const c = raw.replace(/\s+/g, '')
  return c.includes(SWITCH) && c.endsWith('::before')
}

// `weak` reaches every element `strong` does: same target, and its states are
// a subset of strong's, so it is a rule the cascade can set the knob with.
const covers = (weak: Sel, strong: Sel): boolean =>
  weak.sig === strong.sig && weak.states.every((s) => strong.states.includes(s))

// `a` wins over `b` in the cascade: the (a) part first, then (b).
const outweighs = (a: Sel, b: Sel): boolean =>
  a.spec[0] > b.spec[0] || (a.spec[0] === b.spec[0] && a.spec[1] >= b.spec[1])

// The system colours a forced-colours block may draw with, the set
// HearthTokens.test.ts pins the blocks to.
const SYSTEM = [
  'GrayText',
  'CanvasText',
  'Canvas',
  'Highlight',
  'HighlightText',
]

// The fill a rule gives its selector, in the shorthand or the longhand.
const fill = (body: string): string =>
  prop(body, 'background') || prop(body, 'background-color')

// The @media (forced-colors: active) blocks, and whether a rule sits in one.
const FORCED = atRanges('@media (forced-colors: active)')
const inForced = (r: Rule): boolean =>
  FORCED.some(([open, end]) => r.depth > 0 && r.at > open && r.at < end)

// The forced-colours rules that draw the knob, picked by the state one item of
// their selector list carries: the block may write the plain and the checked
// knob in one rule (the brief's spelling) or in two, and a lookup of one must
// not land on the other.
const knobRule = (what: string, has: (s: Sel) => boolean): Rule =>
  single(
    inside('@media (forced-colors: active)', '::before').filter((r) =>
      listItems(r.sel).some((raw) => isKnob(raw) && has(parseSel(raw))),
    ),
    what,
  )

// The enabled field rule: the selector list of 2h.163b.
const enabledField = (): Rule =>
  single(
    TOP.filter((r) =>
      ["input[type='text']", "input[type='password']", 'select'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    "the enabled field rule (input[type='text'], input[type='password'], select)",
  )

const disabledField = (): Rule =>
  single(
    TOP.filter((r) =>
      ['input:disabled', 'select:disabled', 'textarea:disabled'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    'the disabled field rule (input:disabled, select:disabled, textarea:disabled)',
  )

const disabledButton = (): Rule =>
  single(
    TOP.filter(
      (r) =>
        r.compact.includes('.primary:disabled') &&
        r.compact.includes('.secondary:disabled'),
    ),
    'the disabled button rule (.primary:disabled, .secondary:disabled)',
  )

// ── The tokens, for the computed pairs of H2a.9 and H2a.7 ─────────────────

const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

const lightTokens = (): [string, string][] =>
  customProps(topBy(':root', 'the light token block').body)

const darkTokens = (): [string, string][] => {
  const [range] = atRanges('@media (prefers-color-scheme: dark)')
  if (!range)
    throw new Error('there is no @media (prefers-color-scheme: dark) block')
  return customProps(
    single(
      RULES.filter(
        (r) =>
          r.depth > 0 &&
          r.at > range[0] &&
          r.at < range[1] &&
          r.compact === ":root:not([data-theme='light'])",
      ),
      'the dark rule inside the dark block (H4.10: :root:not([data-theme=light]))',
    ).body,
  )
}

// index.css is written with no space between a compound's parts, so a
// descendant selector is given to the matchers with its space dropped.
const tight = (sel: string): string => sel.replace(/ /g, '')

// The value of one token; a miss throws naming the token, so a pair that
// lacks a token goes red on the token, not on NaN.
const valueOf = (pairs: [string, string][], name: string): string => {
  const hits = pairs.filter((p) => p[0] === name).map((p) => p[1])
  if (hits.length !== 1)
    throw new Error(`${name} is declared ${hits.length} times in the block`)
  return hits[0]
}

// WCAG 2.2 relative luminance and contrast ratio, the ratio function of
// HearthTokens.test.ts (the 0.04045 threshold), widened to the composited
// background of H2a.9, whose channels are not a 6-digit hex.
const rgbOf = (hex: string): number[] => {
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
const luminance = (rgb: number[]): number =>
  0.2126 * chan(rgb[0]) + 0.7152 * chan(rgb[1]) + 0.0722 * chan(rgb[2])
const contrast = (fg: number[], bg: number[]): number => {
  const ls = [luminance(fg), luminance(bg)].sort((x, y) => y - x)
  return (ls[0] + 0.05) / (ls[1] + 0.05)
}

// The hatch token composited over a background: each channel
// `a * hatch + (1 - a) * surface`, unrounded (H2a.9).
const hatchOver = (hatch: string, over: string): number[] => {
  const m = /^rgb\((\d+)\s+(\d+)\s+(\d+)\s*\/\s*([\d.]+)\)$/.exec(hatch)
  if (!m) throw new Error(`--hatch is not rgb(R G B / a): "${hatch}"`)
  const a = Number.parseFloat(m[4])
  return rgbOf(over).map(
    (s, i) => a * Number.parseInt(m[i + 1], 10) + (1 - a) * s,
  )
}
// The ratio of one token over the hatch laid on another token, in a theme.
const onHatch = (theme: 'light' | 'dark', fg: string): number => {
  const pairs = theme === 'light' ? lightTokens() : darkTokens()
  return contrast(
    rgbOf(valueOf(pairs, fg)),
    hatchOver(valueOf(pairs, '--hatch'), valueOf(pairs, '--surface')),
  )
}
// The ring's accent on one of the five backgrounds of the H1 brief's P11-P15.
const ringOn = (theme: 'light' | 'dark', bg: string): number => {
  const pairs = theme === 'light' ? lightTokens() : darkTokens()
  return contrast(rgbOf(valueOf(pairs, '--accent')), rgbOf(valueOf(pairs, bg)))
}

const THEMES = ['light', 'dark'] as const

// ── H2a.1: buttons ────────────────────────────────────────────────────────

test('H2a.1: the button rule is --control-h tall with min-width 24px and --radius; .primary, .secondary and .about are pills; .jump button stays 32px', () => {
  const button = topBy('button', 'the button rule')
  expect(
    prop(button.body, 'min-height'),
    'the 40 px control of brief 04 1.1, from --control-h',
  ).toBe('var(--control-h)')
  expect(prop(button.body, 'min-width'), 'kept').toBe('24px')
  expect(
    prop(button.body, 'border-radius'),
    'a square button keeps --radius',
  ).toBe('var(--radius)')
  for (const sel of ['.primary', '.secondary']) {
    const rule = topBy(sel, `the ${sel} rule`)
    expect(
      prop(rule.body, 'border'),
      `${sel}: the 1.5 px border from the tokens, so no literal 1px is left on a control`,
    ).toBe(
      sel === '.primary'
        ? 'var(--border-w) solid var(--accent)'
        : 'var(--border-w) solid var(--control)',
    )
    expect(prop(rule.body, 'border-radius'), `${sel} is a pill`).toBe(
      'var(--radius-pill)',
    )
    expect(prop(rule.body, 'padding'), `${sel} padding`).toBe(
      'var(--space-1) var(--space-4)',
    )
  }
  const about = topBy('.about', 'the .about disclosure')
  expect(prop(about.body, 'border-radius'), '.about is a pill').toBe(
    'var(--radius-pill)',
  )
  expect(
    prop(about.body, 'min-height'),
    '.about keeps 24px: a pill, not a row',
  ).toBe('24px')
  expect(
    prop(
      flatBy('.jump button', 'the jump list button rule').body,
      'min-height',
    ),
    'the jump list keeps 32 px buttons until H9 (section 0)',
  ).toBe('32px')
})

// ── H2a.2: fields ─────────────────────────────────────────────────────────

test('H2a.2: the enabled field and the filter input are --control-h with a --border-w --control border on --field, .field-label lines up at --control-h, input.number keeps 10ch, and an aria-invalid field keeps its --danger border', () => {
  for (const [what, body] of [
    ['the enabled field rule', enabledField().body],
    [
      'the .model-profiles-filter-row input rule',
      flatBy('.model-profiles-filter-row input', 'the filter input').body,
    ],
  ] as [string, string][]) {
    expect(prop(body, 'min-height'), `${what}: 40 px`).toBe('var(--control-h)')
    expect(prop(body, 'border'), `${what}: 1.5 px --control`).toBe(
      'var(--border-w) solid var(--control)',
    )
    expect(prop(body, 'border-radius'), `${what}: --radius`).toBe(
      'var(--radius)',
    )
    expect(prop(body, 'background'), `${what}: --field`).toBe('var(--field)')
  }
  const number = topBy('input.number', 'the boxed number')
  expect(
    prop(number.body, 'width'),
    '10ch, the full-value rule counts 8 characters (DESIGN.md 5.2)',
  ).toBe('10ch')
  expect(prop(number.body, 'flex'), 'and its flex').toBe('0 1 10ch')
  const label = single(
    inside('@media (min-width: 720px)', '.field-label').filter(
      (r) => prop(r.body, 'min-height') !== '',
    ),
    'the .field-label rule with a min-height inside a @media (min-width: 720px) block',
  )
  expect(
    prop(label.body, 'min-height'),
    'the label column is as tall as the control beside it',
  ).toBe('var(--control-h)')
  const invalid = single(
    TOP.filter(
      (r) =>
        r.compact.includes("input[aria-invalid='true']") &&
        r.compact.includes("select[aria-invalid='true']"),
    ),
    "the aria-invalid field rule (input[aria-invalid='true'], select[aria-invalid='true'])",
  )
  expect(
    prop(invalid.body, 'border-color'),
    'a field marked invalid keeps its --danger border colour',
  ).toBe('var(--danger)')
  expect(
    invalid.at > enabledField().at,
    'it is written after the enabled field rule, so the new border shorthand does not cover it',
  ).toBe(true)
})

// ── H2a.3: the checkbox ───────────────────────────────────────────────────

test('H2a.3: the box is 1.25rem with a --border-w border and --radius-check, the tick is 0.6 by 0.32rem drawn by --on-accent borders, and the checked, disabled and aria-disabled looks keep their rules and their order', () => {
  const box = topBy(CHECKBOX, 'the checkbox rule')
  expect(prop(box.body, 'width'), 'the 20 px box').toBe('1.25rem')
  expect(
    prop(box.body, 'height'),
    'a box without text, so a height is allowed',
  ).toBe('1.25rem')
  expect(prop(box.body, 'border'), 'the 1.5 px --control edge').toBe(
    'var(--border-w) solid var(--control)',
  )
  expect(prop(box.body, 'border-radius'), 'the 5 px corner').toBe(
    'var(--radius-check)',
  )
  const tick = topBy(`${CHECKBOX}::before`, 'the tick')
  expect(prop(tick.body, 'width'), 'the tick scaled to 0.6rem').toBe('0.6rem')
  expect(prop(tick.body, 'height'), 'by 0.32rem').toBe('0.32rem')
  expect(prop(tick.body, 'border-left'), 'still drawn by its left border').toBe(
    '2px solid var(--on-accent)',
  )
  expect(prop(tick.body, 'border-bottom'), 'and its bottom border').toBe(
    '2px solid var(--on-accent)',
  )
  const checked = single(
    TOP.filter(
      (r) =>
        r.compact.includes(`${CHECKBOX}:checked`) &&
        !r.compact.includes('::before'),
    ),
    "the checked box's rule",
  )
  expect(prop(checked.body, 'background'), 'the checked box is --accent').toBe(
    'var(--accent)',
  )
  const disabled = single(
    TOP.filter(
      (r) =>
        r.compact.includes(CHECKBOX) &&
        r.compact.includes(':disabled') &&
        r.compact.includes(`${CHECKBOX}[aria-disabled='true']`) &&
        !r.compact.includes('::before'),
    ),
    "the disabled checkbox rule, kept shared with input[type='checkbox'][aria-disabled='true']",
  )
  expect(
    prop(disabled.body, 'background'),
    'the inactive box is --surface',
  ).toBe('var(--surface)')
  expect(
    dashedControl(disabled.body),
    'with a dashed --control border (the shorthand on --border-w, or a dashed style plus the colour)',
  ).toBe(true)
  expect(
    disabled.at > checked.at,
    'the disabled box rule comes after :checked, as 2h.163c pins (2h.163c order kept)',
  ).toBe(true)
  const tickOff = single(
    TOP.filter((r) => r.compact.includes(`${CHECKBOX}:disabled::before`)),
    "the disabled box's tick rule",
  )
  expect(prop(tickOff.body, 'border-color'), 'the tick stays --muted').toBe(
    'var(--muted)',
  )
})

// ── H2a.4: the switch track ───────────────────────────────────────────────

test("H2a.4: input[role='switch'] is a 2.75rem by 1.5rem pill after the checkbox rules, with the checkbox's fills (no background of its own)", () => {
  const track = topBy(
    SWITCH,
    'the switch track rule, added after the checkbox rules',
  )
  expect(prop(track.body, 'width'), '44 px').toBe('2.75rem')
  expect(prop(track.body, 'height'), '24 px, a box without text').toBe('1.5rem')
  expect(prop(track.body, 'border-radius'), 'a pill').toBe('var(--radius-pill)')
  expect(
    prop(track.body, 'place-content'),
    'the knob starts at the left edge',
  ).toBe('center start')
  expect(prop(track.body, 'padding'), 'the 2px inset for the knob').toBe(
    '0 2px',
  )
  expect(
    track.at > topBy(CHECKBOX, 'the checkbox rule').at,
    'the switch rule is written after input[type=checkbox], so the track geometry wins',
  ).toBe(true)
  expect(
    prop(track.body, 'background'),
    'the track declares no fill: it takes the checkbox rules, --field off, --accent checked, --surface disabled (DESIGN.md 5.2: drawn and made inactive exactly as a checkbox)',
  ).toBe('')
})

// ── H2a.5: the switch knob ────────────────────────────────────────────────

test('H2a.5: the knob is a 1.05rem circle in --control, --on-accent and 1.2rem along when checked, --muted when disabled after the checked rule, and nothing moves', () => {
  const knob = topBy(`${SWITCH}::before`, 'the switch knob')
  expect(prop(knob.body, 'visibility'), 'the knob is always drawn').toBe(
    'visible',
  )
  expect(prop(knob.body, 'width'), 'the 16.8 px knob').toBe('1.05rem')
  expect(prop(knob.body, 'height'), 'a circle, a box without text').toBe(
    '1.05rem',
  )
  expect(prop(knob.body, 'margin'), 'no tick offset').toBe('0')
  expect(prop(knob.body, 'border'), 'no border on the knob').toBe('0')
  expect(prop(knob.body, 'border-radius'), 'round').toBe('50%')
  expect(prop(knob.body, 'transform'), 'no tick rotation').toBe('none')
  expect(
    prop(knob.body, 'background'),
    'the off knob is --control (1.4.11: 3:1 on every fill)',
  ).toBe('var(--control)')
  const on = topBy(`${SWITCH}:checked::before`, 'the checked knob')
  expect(prop(on.body, 'background'), 'the on knob is --on-accent').toBe(
    'var(--on-accent)',
  )
  expect(prop(on.body, 'translate'), 'it sits at the right end').toBe(
    '1.2rem 0',
  )
  const off = single(
    TOP.filter(
      (r) =>
        r.compact.includes(`${SWITCH}:disabled::before`) &&
        r.compact.includes(`${SWITCH}[aria-disabled='true']::before`),
    ),
    "the disabled knob rule (input[role='switch']:disabled::before, input[role='switch'][aria-disabled='true']::before)",
  )
  expect(prop(off.body, 'background'), 'the inactive knob is --muted').toBe(
    'var(--muted)',
  )
  expect(
    off.at > on.at,
    'the disabled knob rule is written AFTER the checked one, so a disabled switch that is on keeps a --muted knob',
  ).toBe(true)
  const moving: string[] = []
  for (const r of TOP.filter((r) => r.compact.startsWith(SWITCH)))
    for (const d of r.body.split(';')) {
      const i = d.indexOf(':')
      if (i < 0) continue
      const name = d.slice(0, i).trim()
      if (/^(transition|animation)(-|$)/.test(name))
        moving.push(`${r.sel.replace(/\s+/g, ' ')}: ${name}`)
    }
  expect(
    moving,
    'the knob jumps: no transition on the switch (section 0, H1a.11)',
  ).toEqual([])
})

// ── H2a.6: the switch in forced colours ───────────────────────────────────

test('H2a.6: in forced colours the knob is CanvasText with forced-color-adjust none, GrayText when inactive, and the existing GrayText border rules stay', () => {
  const knob = knobRule(
    "the plain input[role='switch']::before rule inside a @media (forced-colors: active) block (its selector list may carry the checked knob beside it, as the brief writes the pair in one rule)",
    (s) => s.states.length === 1,
  )
  expect(
    prop(knob.body, 'forced-color-adjust'),
    'the knob background would be forced to the track colour and vanish without it',
  ).toBe('none')
  expect(prop(knob.body, 'background'), 'the knob is the text colour').toBe(
    'CanvasText',
  )
  const off = single(
    inside('@media (forced-colors: active)', `${SWITCH}:disabled::before`),
    'the disabled/aria-disabled knob rule inside a @media (forced-colors: active) block',
  )
  expect(prop(off.body, 'background'), 'an inactive knob is GrayText').toBe(
    'GrayText',
  )
  for (const sel of [
    tight(
      ".model-profiles-switch input[type='checkbox'][aria-disabled='true']",
    ),
    tight(".check input[type='checkbox'][aria-disabled='true']"),
  ]) {
    const kept = inside('@media (forced-colors: active)', sel)
    expect(
      kept.length > 0,
      `the forced-colours rule whose selector starts ${sel} stays, so the aria-disabled box keeps a GrayText border`,
    ).toBe(true)
    for (const r of kept)
      expect(
        prop(r.body, 'border-color'),
        `${r.sel.replace(/\s+/g, ' ')}: its GrayText border`,
      ).toBe('GrayText')
  }
})

// The defect the H2a review found in real browsers: the block names the plain
// knob only, and the checked knob outside it outweighs that rule.
test("H2a.6: the forced-colours block answers the checked knob's background too, before the GrayText rule, and every knob background outside with a system colour of at least its specificity", () => {
  const on = knobRule(
    "the rule inside a @media (forced-colors: active) block whose selector list carries input[role='switch']:checked::before",
    (s) => s.states.includes(':checked'),
  )
  expect(
    prop(on.body, 'background'),
    "the knob of a switch that is ON: the outside input[role='switch']:checked::before (spec 2,2: the attribute, :checked, the element, ::before) outweighs the block's plain ::before rule (spec 1,2), so the block must name the checked knob as well, or --on-accent survives on the forced Canvas track, where it measured 1:1: the knob of a switch that is ON vanishes",
  ).toBe('CanvasText')
  const off = knobRule(
    "the rule inside a @media (forced-colors: active) block whose selector list carries input[role='switch']:disabled::before",
    (s) => s.states.includes(':disabled'),
  )
  expect(
    prop(off.body, 'background'),
    'an inactive knob is GrayText (kept)',
  ).toBe('GrayText')
  expect(
    on.at < off.at,
    "the CanvasText rule for the checked knob is written BEFORE the disabled knob's GrayText rule in the block: the disabled rule has the checked rule's specificity and wins by coming after it, so a switch that is off and inactive keeps GrayText",
  ).toBe(true)
  // The same rule stated for every knob background: whatever a rule outside a
  // forced-colours block fills the knob with, a rule inside one must answer it
  // at its own specificity, with a system colour.
  const met: { sel: Sel; colour: string }[] = []
  for (const r of RULES.filter(inForced)) {
    const colour = fill(r.body)
    if (!SYSTEM.includes(colour)) continue
    for (const raw of listItems(r.sel))
      if (isKnob(raw)) met.push({ sel: parseSel(raw), colour })
  }
  const holes: string[] = []
  for (const r of RULES.filter((r) => !inForced(r))) {
    const colour = fill(r.body)
    if (!colour) continue
    for (const raw of listItems(r.sel)) {
      if (!isKnob(raw)) continue
      const s = parseSel(raw)
      const near = met.filter((m) => covers(m.sel, s))
      if (near.some((m) => outweighs(m.sel, s))) continue
      holes.push(
        `${s.raw} (spec ${s.spec.join(',')}) fills ${colour}; the block answers with ` +
          (near.length
            ? near
                .map(
                  (m) =>
                    `${m.sel.raw} (spec ${m.sel.spec.join(',')}) ${m.colour}`,
                )
                .join(', ')
            : 'no rule that reaches the knob'),
      )
    }
  }
  expect(
    holes,
    'every background the knob has outside a forced-colours block is met inside one by a system colour of at least the same specificity: the outside colour survives the forced colours, and the knob is drawn in a colour forced against its track',
  ).toEqual([])
})

// ── H2a.7: the focus ring ─────────────────────────────────────────────────

test('H2a.7: the ring is --ring-w solid --accent with a 2px offset on :focus-visible and .card h3:focus-visible, and the focus the page moves keeps no ring', () => {
  for (const sel of [':focus-visible', '.card h3:focus-visible']) {
    const rule = topBy(tight(sel), `the ${sel} rule`)
    expect(
      prop(rule.body, 'outline'),
      `${sel}: the 3 px accent ring of brief 04 1.1`,
    ).toBe('var(--ring-w) solid var(--accent)')
    expect(prop(rule.body, 'outline-offset'), `${sel}: offset 2px`).toBe('2px')
  }
  expect(
    prop(
      topBy("[tabindex='-1']:focus", "the [tabindex='-1']:focus rule").body,
      'outline',
    ),
    'a focus the page moves shows no ring (2h.130): kept',
  ).toBe('none')
})

test.each(
  THEMES.flatMap((theme) =>
    ['--page', '--bg', '--surface', '--field', '--accent-soft'].map(
      (bg) => [theme, bg] as ['light' | 'dark', string],
    ),
  ),
)(
  'H2a.7: the ring accent on %s clears the 6.04:1 floor in %s (P11 to P15)',
  (theme, bg) => {
    const ratio = ringOn(theme, bg)
    // The brief states 6.04:1, the rounding of the measured 6.0366 of --accent
    // on --accent-soft in light, whose tokens H1a pins: the floor is the
    // measurement truncated, so the pair is pinned green by construction.
    expect(
      ratio,
      `--accent on ${bg} in ${theme}: ${ratio.toFixed(2)}:1, floor 6.04:1 (2.4.7, 1.4.11)`,
    ).toBeGreaterThanOrEqual(6.03)
  },
)

test('H2a.7: the tightest ring pair is the 6.04:1 the brief states (light --accent on --accent-soft)', () => {
  expect(
    ringOn('light', '--accent-soft').toFixed(2),
    'the lowest of P11 to P15 in light, as brief 04 1.1 measures it',
  ).toBe('6.04')
})

// ── H2a.8: the disabled hatch ─────────────────────────────────────────────

test('H2a.8: the disabled field and the disabled button keep --surface, carry the hatch after it in the same block, and take a --border-w dashed --control border', () => {
  for (const [what, rule] of [
    ['the disabled field rule', disabledField()],
    ['the disabled button rule', disabledButton()],
  ] as [string, Rule][]) {
    expect(
      prop(rule.body, 'background'),
      `${what}: the flat --surface fill is kept`,
    ).toBe('var(--surface)')
    expect(
      prop(rule.body, 'background-image'),
      `${what}: the 135 degree hatch`,
    ).toBe(HATCH)
    const bg = rule.body.indexOf('background:')
    const image = rule.body.indexOf('background-image:')
    expect(
      bg >= 0 && image > bg,
      `${what}: the background-image comes AFTER the background shorthand in the same block, which would reset it`,
    ).toBe(true)
    expect(
      dashedControl(rule.body),
      `${what}: the dashed border is the 1.5 px one (the shorthand on --border-w, or a dashed style plus the --control colour)`,
    ).toBe(true)
  }
})

test('section 0: the hatch is on the disabled field and the disabled button only: not the checkbox, not the switch, not the aria-disabled controls', () => {
  const hatched: string[] = []
  for (const r of RULES)
    if (prop(r.body, 'background-image').includes('repeating-linear-gradient'))
      hatched.push(r.sel.replace(/\s+/g, ' '))
  expect(
    hatched.sort(),
    'only the two disabled rules carry the hatch: a 20 px box shows no pattern (a preference)',
  ).toEqual([
    '.primary:disabled, .secondary:disabled',
    'input:disabled, select:disabled, textarea:disabled',
  ])
  const aria = single(
    TOP.filter((r) =>
      r.compact.includes(
        ".primary[aria-disabled='true'],.secondary[aria-disabled='true']",
      ),
    ),
    "the aria-disabled buttons' rule",
  )
  expect(
    prop(aria.body, 'background-image'),
    'the aria-disabled buttons keep their own flat look',
  ).toBe('')
  expect(
    dashedControl(aria.body),
    'and their solid border, not the dashed one (2h.163e)',
  ).toBe(false)
})

// ── H2a.9: the hatched pair ───────────────────────────────────────────────

test('H2a.9: the composite reproduces the measurements of the brief: 4.86 and 5.49 for --muted, 3.05 and 3.21 for --control', () => {
  for (const [theme, fg, want] of [
    ['light', '--muted', '4.86'],
    ['dark', '--muted', '5.49'],
    ['light', '--control', '3.05'],
    ['dark', '--control', '3.21'],
  ] as ['light' | 'dark', string, string][])
    expect(
      onHatch(theme, fg).toFixed(2),
      `${fg} on --hatch over --surface in ${theme}, the brief's measured ${want}`,
    ).toBe(want)
})

test.each(
  THEMES.flatMap(
    (theme) =>
      [
        [theme, '--muted', 4.5],
        [theme, '--control', 3],
      ] as ['light' | 'dark', string, number][],
  ),
)(
  'H2a.9: %s on the hatch over --surface is at least %s:1 in %s',
  (theme, fg, floor) => {
    const ratio = onHatch(theme, fg)
    expect(
      ratio,
      `${fg} on --hatch over --surface in ${theme}: ${ratio.toFixed(2)}:1, floor ${floor}:1 (the tightest pair of Hearth)`,
    ).toBeGreaterThanOrEqual(floor)
  },
)

// ── The sizes the brief names, and the tokens behind them ─────────────────

test.each([
  [
    'button',
    () => prop(topBy('button', 'the button rule').body, 'min-height'),
    'var(--control-h)',
  ],
  [
    'the enabled field rule',
    () => prop(enabledField().body, 'min-height'),
    'var(--control-h)',
  ],
  [
    '.model-profiles-filter-row input',
    () =>
      prop(
        flatBy('.model-profiles-filter-row input', 'the filter input').body,
        'min-height',
      ),
    'var(--control-h)',
  ],
  [
    '.field-label (min-width: 720px)',
    () =>
      prop(
        single(
          inside('@media (min-width: 720px)', '.field-label').filter(
            (r) => prop(r.body, 'min-height') !== '',
          ),
          'the .field-label rule with a min-height in the 720px block',
        ).body,
        'min-height',
      ),
    'var(--control-h)',
  ],
  ['.about', () => prop(topBy('.about', '.about').body, 'min-height'), '24px'],
  ['.check', () => prop(topBy('.check', '.check').body, 'min-height'), '24px'],
  [
    '.model-profiles-switch',
    () =>
      prop(
        topBy('.model-profiles-switch', 'the row switch label').body,
        'min-height',
      ),
    '24px',
  ],
  [
    '.jump button',
    () => prop(flatBy('.jump button', 'the jump rule').body, 'min-height'),
    '32px',
  ],
  ['--control-h in :root', () => valueOf(lightTokens(), '--control-h'), '40px'],
] as [string, () => string, string][])(
  'section 1 sizes: %s',
  (_label, get, want) => {
    expect(get(), `${_label}: the brief pins ${want}`).toBe(want)
  },
)

test('section 1 (2.5.8): the targets keep 24 by 24 px: the button min-width, the switch label and the checkbox row', () => {
  expect(
    prop(topBy('button', 'the button rule').body, 'min-width'),
    'a button is at least 24 px wide',
  ).toBe('24px')
  const box = topBy('.model-profiles-switch', 'the row switch label').body
  expect(prop(box, 'min-width'), 'the switch label is 24 px wide').toBe('24px')
  expect(prop(box, 'min-height'), 'and 24 px tall').toBe('24px')
  expect(
    prop(topBy('.check', '.check').body, 'min-height'),
    'a checkbox row is at least 24 px tall',
  ).toBe('24px')
})

// ── Section 4: text spacing, the one item checkable from the text ─────────

test('section 4 (1.4.12): every fixed height is a box without text, and the controls that hold text are sized by min-height', () => {
  // A height on a box with text clips it when the user widens the text; the
  // drawn boxes (the box, the tick, the track, the knob, the badge shape) and
  // the visually-hidden clip are the only ones with a height. The height lands
  // on EVERY selector of a rule's list, so a rule is exempt only when every
  // one of them is listed, matched WHOLE: a name found inside a selector
  // exempts a box nobody named (`.markdown` holds `.mark`, and a markdown box
  // holds text), so each text-free box writes its own selector, pseudo-element
  // included. A clip box is exempt by its declarations, not by a name: 1px
  // tall, clipped to nothing, overflow hidden, it shows no text, and a rule
  // with only some of the three is not exempt (H5a.5: the theme switch label
  // and the user name are hidden that way below 480 px, so they are not
  // listed).
  const isClipBox = (body: string): boolean =>
    prop(body, 'clip-path') === 'inset(50%)' &&
    prop(body, 'height') === '1px' &&
    prop(body, 'overflow') === 'hidden'
  // The blocks a CSS text holds, at any nesting: the rules of the top level
  // and those inside an at-rule. The at-rule itself is one of them, flagged
  // by the `@` its selector starts with, and it holds no box of its own.
  const blocksIn = (css: string): { selector: string; body: string }[] => {
    const stripped = css.replace(/\/\*[\s\S]*?\*\//g, (m) =>
      ' '.repeat(m.length),
    )
    const out: { selector: string; body: string }[] = []
    const scan = (text: string): void => {
      let i = 0
      for (;;) {
        const open = text.indexOf('{', i)
        if (open < 0) return
        let end = open + 1
        let depth = 1
        while (end < text.length && depth > 0) {
          if (text[end] === '{') depth++
          else if (text[end] === '}') depth--
          if (depth > 0) end++
        }
        if (depth > 0) return
        const selector = text.slice(i, open).trim()
        const body = text.slice(open + 1, end)
        out.push({ selector, body })
        if (selector.startsWith('@')) scan(body)
        i = end + 1
      }
    }
    scan(stripped)
    return out
  }
  const TEXT_FREE = [
    '.visually-hidden',
    '.shape',
    '.shape-failed::before',
    '.shape-failed::after',
    '.mark',
    // The theme switch's icon: an aria-hidden svg with one path, no text, the
    // fixed box of 07-hearth-h4-theme.md H4.11.
    '.theme-switch svg',
    // The icons of 08-hearth-h8-icons.md H8a.4: the svg is aria-hidden and
    // draws, it holds no text (pinned in HearthIcons.test.tsx), so the 1.25em
    // square the brief gives it is a box without text.
    '.icon',
    "input[type='checkbox']",
    "input[type='checkbox']::before",
    "input[role='switch']",
    "input[role='switch']::before",
  ]
  const textFree = (flat: string): boolean =>
    flat
      .split(',')
      .map((s) => s.trim())
      .every((s) => TEXT_FREE.includes(s))
  expect(textFree('.mark'), 'the listed box, matched whole').toBe(true)
  expect(
    textFree('.markdown'),
    'a selector that only holds a listed name is not exempt',
  ).toBe(false)
  expect(
    textFree('.markdown, .mark'),
    'one listed selector does not exempt a list that also carries a text box',
  ).toBe(false)
  const fixed: string[] = []
  for (const r of RULES) {
    const height = prop(r.body, 'height')
    // `height: auto` is the absence of a height: the content sizes the box,
    // exactly what 1.4.12 asks (brief 08 H7.6 spells it for the drawing).
    if (
      height &&
      height.trim() !== 'auto' &&
      !textFree(r.flat) &&
      !isClipBox(r.body)
    )
      fixed.push(`${r.flat}: ${height}`)
  }
  expect(fixed, 'no fixed height outside the boxes that carry no text').toEqual(
    [],
  )
  for (const [what, body] of [
    ['button', topBy('button', 'the button rule').body],
    ['the enabled field rule', enabledField().body],
    ['.about', topBy('.about', '.about').body],
    ['.jump button', flatBy('.jump button', 'the jump list button rule').body],
  ] as [string, string][])
    expect(
      prop(body, 'height'),
      `${what}: a min-height, never a height, so the text it holds can grow (1.4.12)`,
    ).toBe('')

  // The same check over any CSS text, so the check itself is testable: text
  // written here, and the stylesheet with its @media blocks, which the rule
  // list above never reads (those rules sit at depth 1).
  const heightsInCss = (css: string): string[] => {
    const out: string[] = []
    for (const b of blocksIn(css)) {
      const height = prop(b.body, 'height')
      if (
        height &&
        height.trim() !== 'auto' &&
        !b.selector.startsWith('@') &&
        !textFree(b.selector) &&
        !isClipBox(b.body)
      )
        out.push(`${b.selector}: ${height}`)
    }
    return out
  }
  // The clip box the stylesheet ships below 480 px (H5a.5), and the same
  // name with a height of its own. A box exempt by its NAME alone is the
  // hole: the theme switch's label is text the user reads.
  const CLIP = `@media (max-width: 479.98px) {
  .theme-switch-label,
  .session > span {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
}`
  expect(
    heightsInCss('.theme-switch-label { height: 20px; padding: 4px }'),
    'a label 20px tall: text at 200% is clipped, so this is the violation',
  ).toEqual(['.theme-switch-label: 20px'])
  expect(
    heightsInCss('.theme-switch-label { min-height: 20px }'),
    'a box that carries no height is not listed',
  ).toEqual([])
  expect(
    heightsInCss('.theme-switch-label { height: auto }'),
    'auto is no height of its own: the box is sized by its content, so it is not listed (brief 08 H7.6)',
  ).toEqual([])
  expect(
    heightsInCss(CLIP),
    'a 1px box clipped to nothing shows no text: exempt by its declarations',
  ).toEqual([])
  expect(
    heightsInCss('.theme-switch-label { height: 1px; overflow: hidden }'),
    'a box one declaration short of a clip box is not exempt: the name alone is not',
  ).toEqual(['.theme-switch-label: 1px'])
  expect(
    heightsInCss(
      '@media (max-width: 480px) { .theme-switch-label { height: 20px } }',
    ),
    'a height inside a media rule is read, not skipped',
  ).toEqual(['.theme-switch-label: 20px'])
  expect(
    heightsInCss(CSS),
    'no fixed height on a box with text, the media rules included',
  ).toEqual([])
})
