// H5c, the three carried issues of the Hearth shell
// (fe/briefs/07-hearth-h5-shell.md, section 3). H5c.1 and H5c.2 are two CSS
// rules: the controls of the sticky save bar cancel, with an equal negative
// scroll-margin-bottom, the scroll-padding-bottom the bar puts on the page, so
// focusing Discard (a control that always sits inside that padded strip) does
// not scroll the page. jsdom does no layout, so these tests pin the stylesheet
// text the way HearthH2a.test.ts, HearthH4.test.ts, HearthH5a.test.ts,
// HearthH5b.test.ts and DisabledLook.test.ts do (comments dropped, the same
// rule regex, the same nesting depth, prop() the last value of a property in a
// block); none of them exports a helper, so the reader is a small local copy.
// H5c.3 (no scroll when Discard takes the focus), H5c.4 (the About focus clears
// the bar) and H5c.5 (the browser's own text at 200 %, 32 px initial size) are
// measurements: the designer's UX pass, as the brief says.
// The two corrections the H5b review asked for are stylesheet text too, so they
// join here: `.session` keeps its items at the right edge when the session part
// wraps (the H5b.0 correction), and the current nav entry takes the system
// highlight in forced colours, as the pressed theme button does.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

// The sheet as written, comments kept: a prettier-ignore comment is a comment,
// so it is gone from the text the rules are scanned from.
const RAW = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')

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
  // combinator (".save-bar button"), which compact cannot.
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
    `the top-level rule whose selector is exactly ${compact} (${what})`,
  )

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

/** The one block written with this at-rule header; a miss names the header. */
const oneAt = (needle: string): [number, number] => {
  const at = CSS.indexOf(needle)
  if (at < 0) throw new Error(`no ${needle} block in index.css`)
  return [at, rangeEnd(at)]
}

/** The rules inside the at-rule block that starts at `open`. */
const inside = (range: [number, number]): Rule[] =>
  RULES.filter((r) => r.depth > 0 && r.at > range[0] && r.at < range[1])

/** The rules inside every block of `ranges`. */
const insideAll = (ranges: [number, number][]): Rule[] =>
  ranges.flatMap((range) => inside(range))

// Every block written with this at-rule header, as [open, end] offsets: there
// are four @media (forced-colors: active) blocks, so a locator takes all.
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

// The text between the braces of the top-level rule Prettier writes with `sel`
// at the start of a line: the sheet is Prettier-formatted, so a nested rule is
// indented and cannot be taken for the top-level one.
const rawTopRule = (sel: string): string => {
  const open = RAW.indexOf(`\n${sel} {`)
  if (open < 0)
    throw new Error(`no top-level "${sel} {" rule at a line start in index.css`)
  const brace = RAW.indexOf('{', open)
  return RAW.slice(brace + 1, RAW.indexOf('}', brace))
}

// The two halves of the rule the brief pins: the padding on the page, and the
// margin on the bar's controls that cancels it.
const PAD = 'scroll-padding-bottom'
const MARGIN = 'scroll-margin-bottom'
const BAR_BUTTON = '.save-bar button'

// ── H5c.1: the bar's controls cancel the padding ────────────────────────

test('H5c.1: the top-level .save-bar button rule cancels the page scroll padding with an equal negative scroll-margin-bottom, written after the html:has(.save-bar) rule, prettier-ignore as that one', () => {
  const bar = topBy('html:has(.save-bar)', 'the html:has(.save-bar) rule')
  const pad = prop(bar.body, PAD)
  const btn = topBy(
    '.save-barbutton',
    'the .save-bar button rule (the bar of H5c.1 is the sticky bar of 2h.135, and its controls are the buttons inside it)',
  )
  // The padding's inner expression, its own calc() unwrapped: the margin's
  // calc multiplies that by -1, so nesting the padding's calc inside would
  // ask for calc(-1 * (calc(...))) and no sheet is written that way.
  const inner = pad.replace(/^calc\((.*)\)$/, '$1')
  expect(
    prop(btn.body, MARGIN),
    'the padding of the rule above, negated: a control inside the sticky bar always lies in the padded strip, so the browser scrolls toward it when it takes the focus (measured: Chromium 422 to 451 px, WebKit 100 to 147 px). An equal negative margin moves the control out of that strip; CSS Scroll Snap 1 5.1 ties scroll-margin to the scroll-into-view operations, and the UX pass measures the focus move in all three engines',
  ).toBe(`calc(-1 * (${inner}))`)
  expect(
    prop(btn.body, MARGIN),
    'the expression of the brief, the same calc as the padding with -1 *',
  ).toBe(
    'calc(-1 * (max(var(--save-bar-height, 7rem), 7rem) + var(--space-3)))',
  )
  expect(
    btn.at > bar.at,
    'written after the html:has(.save-bar) rule, as the brief says (the two are unrelated selectors, so this is the order the sheet reads in)',
  ).toBe(true)
  const raw = rawTopRule(BAR_BUTTON)
  expect(
    raw.includes('/* prettier-ignore */'),
    'the long calc is pinned as written, prettier-ignore like the padding declaration it cancels (2h.135): Prettier would break the calc across lines',
  ).toBe(true)
  expect(
    raw.indexOf('/* prettier-ignore */') < raw.indexOf(MARGIN),
    'the comment sits on the line above the declaration, as it does in the padding rule',
  ).toBe(true)
})

// ── H5c.2: not in the short viewport ───────────────────────────────────

test('H5c.2: inside @media (max-height: 31.98rem) the .save-bar button rule zeroes the margin, after the top-level one and beside the padding the block already zeroes', () => {
  const short = oneAt('@media (max-height: 31.98rem)')
  const btn = single(
    inside(short).filter((r) => r.compact === '.save-barbutton'),
    'the .save-bar button rule inside the short-viewport block (the bar is static there, so a negative margin would let a focused button end below the viewport)',
  )
  expect(
    prop(btn.body, MARGIN),
    'the bar is static and the padding is 0 there: the cancel is cancelled too',
  ).toBe('0')
  expect(
    btn.at > topBy('.save-barbutton', 'the top-level .save-bar button rule').at,
    'after the top-level rule: both are .save-bar button, so the sheet order decides which wins inside the block',
  ).toBe(true)
  const page = single(
    inside(short).filter((r) => r.compact === 'html:has(.save-bar)'),
    'the html:has(.save-bar) rule inside the short-viewport block (2h.135)',
  )
  expect(
    prop(page.body, PAD),
    'the 2h.135 padding is still zeroed there: this slice does not move it',
  ).toBe('0')
})

// ── The H5b.0 correction (from the H5b review): the session part too ───

test('H5b.0, as the H5b review corrects it: .session declares justify-content: flex-end, so the user name and Sign out stay at the right when the session part wraps', () => {
  const session = topBy('.session', 'the .session rule')
  expect(
    prop(session.body, 'justify-content'),
    'measured 2026-11-18 in Chromium, Firefox and WebKit: .session is itself a wrapping flex row wider than the line at 480 px signed in, so its items wrap to the LEFT inside it (the name at x 16 on the third row, Sign out at x 16 on the fourth). .banner-tools alone does not fix that: this one declaration keeps the name and Sign out at the right edge, the DOM and the Tab order unchanged',
  ).toBe('flex-end')
})

// ── The current entry in forced colours (from the H5b review) ──────────

test('H5b.3 in forced colours: the current entry of the nav (the same three selectors) takes the Highlight fill and the HighlightText label with no colour adjust, inside an existing forced-colours block: the four stay four, as the pressed theme button', () => {
  const blocks = atRanges('@media (forced-colors: active)')
  expect(
    blocks.length,
    'the four forced-colours blocks of H1a, H2a.6, H2a and H2b: the new rule joins one of them, it does not open a fifth (section 5: the existing forced-colours blocks do not move)',
  ).toBe(4)
  const rules = insideAll(blocks).filter((r) =>
    r.compact.includes("[aria-current='page']"),
  )
  expect(
    rules.length,
    'exactly one forced-colours rule for the current entry: the fill of the current page goes with the forced palette, and the weight alone is thin, so the entry takes the system highlight there, as the bar colour did before H5b',
  ).toBe(1)
  const forced = rules[0]!
  expect(
    listItems(forced.sel),
    'the same three selectors as the current-entry rule: the nav entry of the page, its aria-current="true" spelling for a page below a nav entry, and the banner Sign in link',
  ).toEqual([
    ".nav a[aria-current='page']",
    ".nav a[aria-current='true']",
    "a.session[aria-current='page']",
  ])
  expect(
    listItems(forced.sel),
    'and unchanged in the top-level rule the forced rule mirrors',
  ).toEqual(
    listItems(
      single(
        TOP.filter(
          (r) =>
            r.compact.includes("[aria-current='page']") &&
            r.compact.includes("[aria-current='true']"),
        ),
        "the top-level current-entry rule (.nav a[aria-current='page'], .nav a[aria-current='true'], a.session[aria-current='page'])",
      ).sel,
    ),
  )
  expect(
    prop(forced.body, 'forced-color-adjust'),
    'the author colours of the current state survive the forced palette',
  ).toBe('none')
  expect(
    prop(forced.body, 'background'),
    'the system highlight is the fill',
  ).toBe('Highlight')
  expect(
    prop(forced.body, 'color'),
    'the label is the text colour of that highlight',
  ).toBe('HighlightText')
  const pressed = single(
    insideAll(blocks).filter((r) =>
      r.compact.includes(".theme-switch[aria-pressed='true']"),
    ),
    "the .theme-switch[aria-pressed='true'] rule inside a forced-colours block (H4.11: the rule this one mirrors)",
  )
  expect(
    ['forced-color-adjust', 'background', 'color'].map((p) =>
      prop(forced.body, p),
    ),
    'the same three declarations as the pressed theme button, so the current entry and a pressed control read as the same system state there',
  ).toEqual(
    ['forced-color-adjust', 'background', 'color'].map((p) =>
      prop(pressed.body, p),
    ),
  )
})
