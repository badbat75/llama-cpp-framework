// The disabled look of the controls (fe/briefs/02h-model-profile-save.md, criteria
// 163a to 163g, DESIGN.md 5.2 "Disabled looks different from enabled"): a
// disabled field, checkbox and button are drawn on --surface with a dashed
// --control border, the value text in --muted, the cursor not-allowed, no
// opacity; the aria-disabled controls keep their own look. jsdom does no
// layout, so this pins the CSS rule itself, as 2h.135 and 2e.118 do: read
// index.css, drop the comments, collect the rules whose selector names the
// selector, assert its declarations.
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
  // combinator (".check input[...]"), which compact cannot.
  flat: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the regex above also captures the rules nested in an
// @media block (their selector text names no at-rule), and the order of the
// rules inside a block says nothing about the cascade of the top-level ones,
// so a count or an order of top-level rules ignores them.
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

// The 1px dashed --control border, in the shorthand or split into a dashed
// style and the colour (both spellings are 163a's).
const dashedControl = (body: string): boolean =>
  prop(body, 'border') === '1px dashed var(--control)' ||
  (prop(body, 'border-style') === 'dashed' &&
    prop(body, 'border-color') === 'var(--control)')

const one = (list: Rule[], what: string): Rule => {
  expect(list.length, `exactly one rule: ${what}`).toBe(1)
  return list[0]
}

const CHECKBOX = "input[type='checkbox']"
const ARIA_BOX = "input[type='checkbox'][aria-disabled='true']"

test('2h.163a: one rule for input, select and textarea :disabled draws the field on --surface with a dashed --control border, --muted text, not-allowed and opacity 1', () => {
  const rule = one(
    TOP.filter((r) =>
      ['input:disabled', 'select:disabled', 'textarea:disabled'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    'the disabled field rule, its selector list holding input:disabled, select:disabled and textarea:disabled',
  )
  expect(prop(rule.body, 'background'), 'on --surface').toBe('var(--surface)')
  expect(
    dashedControl(rule.body),
    'a 1px dashed border in var(--control)',
  ).toBe(true)
  expect(prop(rule.body, 'color'), 'the value text in --muted').toBe(
    'var(--muted)',
  )
  expect(
    prop(rule.body, '-webkit-text-fill-color'),
    'Safari greys the text without it',
  ).toBe('var(--muted)')
  expect(prop(rule.body, 'cursor'), 'the not-allowed cursor').toBe(
    'not-allowed',
  )
  expect(prop(rule.body, 'opacity'), 'no dimming').toBe('1')
})

test('2h.163b and 2h.163f: the enabled field is on --field with a solid --control border and the disabled rule wins by order; no :disabled rule dims and :root declares the Hearth token set', () => {
  const enabled = one(
    TOP.filter((r) =>
      ["input[type='text']", "input[type='password']", 'select'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    "the enabled field rule (input[type='text'], input[type='password'], select)",
  )
  expect(
    prop(enabled.body, 'background'),
    'the enabled field is on --field',
  ).toBe('var(--field)')
  expect(
    prop(enabled.body, 'border'),
    'enabled keeps its solid --control border',
  ).toBe('1px solid var(--control)')
  const dimmed: string[] = []
  for (const r of RULES.filter((r) => r.compact.includes(':disabled')))
    for (const d of r.body.split(';')) {
      const i = d.indexOf(':')
      if (i < 0 || d.slice(0, i).trim() !== 'opacity') continue
      if (!(Number.parseFloat(d.slice(i + 1)) >= 1))
        dimmed.push(`${r.sel}: ${d.trim()}`.replace(/\s+/g, ' '))
    }
  expect(dimmed, 'no rule matching :disabled sets opacity below 1').toEqual([])
  const tokens = new Set<string>()
  for (const r of RULES.filter((r) => r.compact.includes(':root')))
    for (const m of r.body.matchAll(/(--[a-z0-9-]+)\s*:/g)) tokens.add(m[1])
  expect(
    [...tokens].sort(),
    ':root declares the Hearth token set of brief 06 section 1, in light and in dark',
  ).toEqual(
    [
      '--accent',
      '--accent-soft',
      '--bg',
      '--border-w',
      '--control',
      '--control-h',
      '--danger',
      '--field',
      '--font',
      '--fs-body',
      '--fs-h1',
      '--fs-h2',
      '--fs-lg',
      '--fs-md',
      '--fs-sm',
      '--fs-xs',
      '--hatch',
      '--lh',
      '--line',
      '--mono',
      '--muted',
      '--on-accent',
      '--page',
      '--radius',
      '--radius-card',
      '--radius-check',
      '--radius-pill',
      '--ring-w',
      '--shadow',
      '--space-1',
      '--space-2',
      '--space-3',
      '--space-4',
      '--space-5',
      '--space-6',
      '--space-7',
      '--success',
      '--surface',
      '--text',
      '--warning',
    ].sort(),
  )
  const disabled = one(
    TOP.filter((r) =>
      ['input:disabled', 'select:disabled', 'textarea:disabled'].every((s) =>
        r.compact.includes(s),
      ),
    ),
    'the disabled field rule',
  )
  // Only the order is pinned: the more-specific spelling of the selector
  // (input[type='text']:disabled) is the other way 163b allows, not tested.
  expect(
    disabled.at > enabled.at,
    'the disabled field rule comes after the enabled one, so it wins at equal specificity',
  ).toBe(true)
})

test("2h.163c: the disabled checkbox (shared with aria-disabled) is on --surface with a dashed --control border, keeps the --muted tick and outweighs :checked's --accent fill", () => {
  const box = one(
    TOP.filter(
      (r) =>
        r.compact.includes(CHECKBOX) &&
        r.compact.includes(':disabled') &&
        r.compact.includes(ARIA_BOX) &&
        !r.compact.includes('::before'),
    ),
    "the disabled checkbox rule, kept shared with input[type='checkbox'][aria-disabled='true']",
  )
  expect(prop(box.body, 'background'), 'the box on --surface').toBe(
    'var(--surface)',
  )
  expect(dashedControl(box.body), 'a dashed border in var(--control)').toBe(
    true,
  )
  const tick = one(
    TOP.filter((r) =>
      r.compact.includes("input[type='checkbox']:disabled::before"),
    ),
    "the disabled box's ::before (the tick)",
  )
  expect(prop(tick.body, 'border-color'), 'the tick stays --muted').toBe(
    'var(--muted)',
  )
  const checked = one(
    TOP.filter(
      (r) =>
        r.compact.includes("input[type='checkbox']:checked") &&
        !r.compact.includes('::before'),
    ),
    "the checked box's rule",
  )
  expect(
    box.at > checked.at,
    'the disabled box rule comes after :checked, so a disabled ticked box is not filled with --accent',
  ).toBe(true)
  const accentFill = RULES.filter(
    (r) =>
      r.compact.includes(':disabled') &&
      r.compact.includes(':checked') &&
      prop(r.body, 'background') === 'var(--accent)',
  ).map((r) => r.sel)
  expect(
    accentFill,
    'no rule fills a disabled checked box with --accent',
  ).toEqual([])
})

test("2h.163d: .check:has(> input:disabled) dims the label to --muted and the disabled box shows not-allowed, while the aria-disabled box keeps cursor default and its label's full colour", () => {
  const label = one(
    TOP.filter((r) => r.compact.includes('.check:has(>input:disabled)')),
    'the .check:has(> input:disabled) rule',
  )
  expect(
    prop(label.body, 'color'),
    'the text beside a disabled box is --muted',
  ).toBe('var(--muted)')
  const box = one(
    TOP.filter(
      (r) =>
        r.compact.includes(CHECKBOX) &&
        r.compact.includes(':disabled') &&
        !r.compact.includes('::before'),
    ),
    'the disabled checkbox rule',
  )
  expect(prop(box.body, 'cursor'), 'a disabled box shows not-allowed').toBe(
    'not-allowed',
  )
  const aria = one(
    TOP.filter((r) => r.flat.includes(".check input[aria-disabled='true']")),
    "the .check input[aria-disabled='true'] rule",
  )
  expect(
    prop(aria.body, 'cursor'),
    'the aria-disabled box keeps cursor default',
  ).toBe('default')
  const dimLabel = TOP.filter(
    (r) =>
      r.compact.includes(".check:has(>input[aria-disabled='true'])") &&
      prop(r.body, 'color') === 'var(--muted)',
  ).map((r) => r.sel)
  expect(
    dimLabel,
    'no .check:has(> input[aria-disabled=true]) rule puts the label in --muted',
  ).toEqual([])
})

test('2h.163e: a disabled .primary or .secondary is on --surface with a dashed --control border, --muted text and not-allowed, while the aria-disabled buttons keep a solid --control border on --bg', () => {
  const button = one(
    TOP.filter(
      (r) =>
        r.compact.includes('.primary:disabled') &&
        r.compact.includes('.secondary:disabled'),
    ),
    'the disabled button rule (.primary:disabled, .secondary:disabled)',
  )
  expect(prop(button.body, 'background'), 'the button on --surface').toBe(
    'var(--surface)',
  )
  expect(dashedControl(button.body), 'a dashed border in var(--control)').toBe(
    true,
  )
  expect(prop(button.body, 'color'), 'the label in --muted').toBe(
    'var(--muted)',
  )
  expect(prop(button.body, 'cursor'), 'the not-allowed cursor').toBe(
    'not-allowed',
  )
  const aria = one(
    TOP.filter((r) =>
      r.compact.includes(
        ".primary[aria-disabled='true'],.secondary[aria-disabled='true']",
      ),
    ),
    "the aria-disabled buttons' rule (save bar, run controls)",
  )
  expect(prop(aria.body, 'background'), 'aria-disabled stays on --bg').toBe(
    'var(--bg)',
  )
  expect(
    prop(aria.body, 'border-color'),
    'with the --control border colour',
  ).toBe('var(--control)')
  expect(prop(aria.body, 'cursor'), 'and cursor default').toBe('default')
  expect(
    dashedControl(aria.body),
    'the aria-disabled button keeps a solid border, not the dashed one',
  ).toBe(false)
})

// The save bar's scroll padding (DESIGN.md 5.2 "Sticky save bar", WCAG 2.4.11
// Focus Not Obscured): the scroll container pads by the bar's rendered height,
// which the bar measures as it grows (an alert and its button can triple it).
// Before that measurement lands, a field focused near the bottom must still
// clear the bar, so the declaration carries a floor of its own. jsdom does no
// layout, so as 2h.135 and 2e.118 do, this pins the CSS text.
test('DESIGN.md 5.2: the save-bar scroll padding has a 7rem floor inside max(), and the short-viewport media rule still zeroes it', () => {
  const PAD = 'scroll-padding-bottom'
  const bar = one(
    TOP.filter((r) => r.compact.includes('html:has(.save-bar)')),
    'the top-level html:has(.save-bar) rule',
  )
  expect(
    prop(bar.body, PAD),
    'the floor of the height a bar with a notice needs, so a focused field clears it before the measured height grows',
  ).toBe('calc(max(var(--save-bar-height, 7rem), 7rem) + var(--space-3))')
  // compact is the selector text only, so a nested rule cannot name the
  // at-rule around it: locate the media block in the CSS text by its own
  // matched braces, then take the rule the scanner found inside that range.
  const open = CSS.indexOf('@media (max-height: 31.98rem)')
  expect(open, 'the short-viewport media block').toBeGreaterThan(-1)
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) break
  }
  const short = one(
    RULES.filter(
      (r) =>
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.sel === 'html:has(.save-bar)',
    ),
    'the short-viewport html:has(.save-bar) rule, inside the media block',
  )
  expect(prop(short.body, PAD), 'the bar is not sticky there: no padding').toBe(
    '0',
  )
  expect(
    short.at > bar.at,
    'the media rule comes after the top-level one, so it wins there',
  ).toBe(true)
})

// The nav's current entry (fe/DESIGN.md, 2.3): a page below a nav entry that
// has no link of its own (a model profile's page under Model profiles) keeps
// that entry in the current-page style with `aria-current="true"`, so the rule
// that draws the style must cover both values, as one rule: the two spellings
// of the same state are the same look. jsdom does no layout, so as 2h.135 and
// the tests above do, this pins the CSS text.
test("DESIGN.md 2.3: the rule that styles the nav's current entry covers aria-current='page' and aria-current='true' as one rule, with the same declarations", () => {
  const current = one(
    TOP.filter((r) => r.compact.includes(".nava[aria-current='page']")),
    "the top-level rule that styles the nav's current entry (.nav a[aria-current='page'])",
  )
  expect(
    current.compact.includes(".nava[aria-current='true']"),
    "the same selector list holds .nav a[aria-current='true']: the entry of a page below it is drawn as the current one too",
  ).toBe(true)
  const stylesTrue = TOP.filter((r) =>
    r.compact.includes("[aria-current='true']"),
  ).map((r) => r.sel.replace(/\s+/g, ' '))
  expect(
    stylesTrue,
    "one rule only names aria-current='true', so both values get the same declarations: not a second rule that draws true with others",
  ).toHaveLength(1)
  expect(
    prop(current.body, 'color'),
    'the current-page look: --accent text',
  ).toBe('var(--accent)')
  expect(prop(current.body, 'font-weight'), 'and the bolder label').toBe('600')
  expect(prop(current.body, 'box-shadow'), 'and the bar at its left').toBe(
    'inset 3px 0 0 var(--accent)',
  )
})

// The jump list's landing (fe/briefs/02h-model-profile-save.md, 2h.196): a jump button moves
// focus to the card's h3 and scrolls it into view. The heading lands at the viewport's
// top edge, where the ring's top line, with its offset, was cut off (2026-11-18 review),
// so the rule keeps room above it. jsdom does no layout, so as 2h.135, 2e.118 and the
// scroll-padding test above do, this pins the CSS text.
test('2h.196: .card h3 declares scroll-margin-top of var(--space-3), so a jump leaves room above the focus ring', () => {
  const heading = one(
    TOP.filter((r) => r.compact === '.cardh3'),
    'the top-level .card h3 rule, the selector of the jump landing (not .card h3:focus-visible)',
  )
  expect(
    prop(heading.body, 'scroll-margin-top'),
    'room above the heading so the whole focus ring is visible',
  ).toBe('var(--space-3)')
})
