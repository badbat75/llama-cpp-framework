// H5b, the nav of the Hearth shell (fe/briefs/07-hearth-h5-shell.md, section 2,
// criteria H5b.0 to H5b.6): a restyle of the shell's rules, CSS only, so these
// tests pin the stylesheet text. jsdom does no layout, so index.css is read the
// way HearthH2a.test.ts, HearthH4.test.ts, HearthH5a.test.ts and
// DisabledLook.test.ts do (comments dropped, the same rule regex, the same
// nesting depth, prop() the last value of a property in a block); those files
// export no helper, so the reader is a small local copy. The DOM and the Menu's
// behaviour (H5b.7) are in HearthNav.test.tsx; the pills, the current fill, the
// group rules, the 320 px menu with its --line end and the focus ring on a
// current pill are the designer's UX pass (brief section 4), as is the
// sideways-scroll and text-spacing half of H5b.6.
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
  // combinator (".nav ul[aria-labelledby]"), which compact cannot.
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

// ── H5b.0: the tools stay right-aligned when they wrap ──────────────────

test('H5b.0: .banner-tools also declares justify-content: flex-end, so a wrapped tools group keeps its items at the right edge', () => {
  const tools = topBy('.banner-tools', 'the .banner-tools rule')
  expect(
    prop(tools.body, 'justify-content'),
    'measured 2026-11-18 in all three engines: when the group is wider than what is left on the name row it takes a whole row and its items wrap to the LEFT inside it (Menu at x 16, Sign out on a third row at x 16), against section 0 of the brief: this one declaration is what keeps them right-aligned at every width',
  ).toBe('flex-end')
})

// ── H5b.1: the nav sits on the paper ────────────────────────────────────

test('H5b.1: .nav is transparent, padded --space-3 --space-4 --space-5 with a 1px solid --line bottom edge, and the wide block keeps border-bottom: 0 and declares no border-right', () => {
  const nav = topBy('.nav', 'the .nav rule')
  expect(
    prop(nav.body, 'background'),
    'transparent: the shell sits on the page with the banner and the content cards lift off it (brief 04 1.2)',
  ).toBe('transparent')
  expect(prop(nav.body, 'padding'), 'the padding of the brief').toBe(
    'var(--space-3) var(--space-4) var(--space-5)',
  )
  expect(
    prop(nav.body, 'border-bottom'),
    'one decorative edge at --line, not --control: seen only below 960 px when the menu is open, so the menu end reads (the --line of a decoration needs no contrast, brief 04 criterion 4)',
  ).toBe('1px solid var(--line)')
  const wide = single(
    inside(oneAt('@media (min-width: 960px)')).filter(
      (r) => r.compact === '.nav',
    ),
    'the .nav rule inside the @media (min-width: 960px) block',
  )
  expect(
    prop(wide.body, 'border-bottom'),
    'the wide nav is a column: it keeps border-bottom: 0, so the --line edge shows only in the narrow menu',
  ).toBe('0')
  expect(
    ['', '0'].includes(prop(wide.body, 'border-right')),
    'and no column divider: no border-right declaration, or border-right: 0',
  ).toBe(true)
})

// ── H5b.2: the entries are pills ────────────────────────────────────────

test('H5b.2: .nav a is a pill (a flex row centred, min-height 32px, padded --space-1 --space-4, --radius-pill) in --text with no underline and the long-id break kept, and .nav a:hover keeps the underline', () => {
  const link = topBy('.nava', 'the .nav a rule')
  expect(
    prop(link.body, 'display'),
    'a row, so the label sits centred in the pill',
  ).toBe('flex')
  expect(prop(link.body, 'align-items'), 'centred').toBe('center')
  expect(prop(link.body, 'min-height'), 'the 32 px pill (2.5.8 asks 24)').toBe(
    '32px',
  )
  expect(
    prop(link.body, 'padding'),
    'the pill padding, --space-4 so the label lines up with the group label',
  ).toBe('var(--space-1) var(--space-4)')
  expect(
    prop(link.body, 'border-radius'),
    '--radius-pill, like a.session and the controls',
  ).toBe('var(--radius-pill)')
  expect(prop(link.body, 'color'), 'the text colour is kept').toBe(
    'var(--text)',
  )
  expect(
    prop(link.body, 'text-decoration'),
    'no underline on an entry (section 5: the names and the rules H2 left do not move)',
  ).toBe('none')
  expect(prop(link.body, 'overflow-wrap'), 'the long-id break is kept').toBe(
    'anywhere',
  )
  expect(
    prop(topBy('.nava:hover', 'the .nav a:hover rule').body, 'text-decoration'),
    'the hover underline stays: nav links are links, not controls, so H2 does not apply (section 0); the mockup --surface hover fill is not taken',
  ).toBe('underline')
})

// ── H5b.3: the current entry is a --accent-soft pill ────────────────────

test("H5b.3: the one current-entry rule (the unchanged selector list of aria-current='page' and 'true') fills --accent-soft with --accent text at weight 600 and declares no box-shadow", () => {
  const current = single(
    TOP.filter(
      (r) =>
        r.compact.includes("a[aria-current='page']") &&
        r.compact.includes("a[aria-current='true']"),
    ),
    "the current-entry rule (.nav a[aria-current='page'], .nav a[aria-current='true'], a.session[aria-current='page'])",
  )
  expect(
    listItems(current.sel),
    'the selector list of DESIGN.md 2.3, unchanged: the entry of a page below a nav entry (a model profile page) is drawn as the current one too, and the banner Sign in link takes the same pill (H5a.4)',
  ).toEqual([
    ".nav a[aria-current='page']",
    ".nav a[aria-current='true']",
    "a.session[aria-current='page']",
  ])
  expect(
    prop(current.body, 'background'),
    'the --accent-soft fill replaces the inset bar (--accent on --accent-soft is P15, 6.04 / 6.93, already pinned by HearthTokens.test.ts: 1.4.3)',
  ).toBe('var(--accent-soft)')
  expect(prop(current.body, 'color'), 'the accent text stays').toBe(
    'var(--accent)',
  )
  expect(
    prop(current.body, 'font-weight'),
    '600, not the mockup 700, so the 600 pin holds (rendered the same by Atkinson, brief 06 H1b): the weight is the non-colour cue of 1.4.1, and in forced colours the fill goes and the weight stays',
  ).toBe('600')
  expect(
    prop(current.body, 'box-shadow'),
    'no inset bar: no box-shadow declaration at all',
  ).toBe('')
  const namesTrue = TOP.filter((r) =>
    r.compact.includes("[aria-current='true']"),
  ).map((r) => r.sel.replace(/\s+/g, ' '))
  expect(
    namesTrue,
    "this stays the only rule naming aria-current='true', so both spellings of the state keep the same declarations (DESIGN.md 2.3)",
  ).toHaveLength(1)
})

// ── H5b.4: the group labels ─────────────────────────────────────────────

test('H5b.4: .nav-label is --fs-xs in --muted at weight 600, with margin --space-4 0 --space-1 and padding 0 --space-4 so its text lines up with the pills', () => {
  const label = topBy('.nav-label', 'the .nav-label rule')
  expect(
    prop(label.body, 'font-size'),
    '--fs-xs: a label, smaller than the entries it names',
  ).toBe('var(--fs-xs)')
  expect(
    prop(label.body, 'color'),
    '--muted on the paper: P6, 5.65 / 7.52',
  ).toBe('var(--muted)')
  expect(prop(label.body, 'font-weight'), 'the weight of a label').toBe('600')
  expect(
    prop(label.body, 'margin'),
    'room above it, none of the label gap of the old rule',
  ).toBe('var(--space-4) 0 var(--space-1)')
  expect(
    prop(label.body, 'padding'),
    '--space-4, the pills padding, so the label text starts where the pill text does',
  ).toBe('0 var(--space-4)')
})

// ── H5b.5: the group rule ───────────────────────────────────────────────

test('H5b.5: .nav ul[aria-labelledby] carries the group rule (margin-left --space-3, padding-left --space-2, border-left 2px solid --line), and no rule matching the bare .nav ul of the Overview list declares it', () => {
  const group = topBy(
    '.navul[aria-labelledby]',
    'the .nav ul[aria-labelledby] rule (the group rule)',
  )
  expect(prop(group.body, 'margin-left'), 'the group indented').toBe(
    'var(--space-3)',
  )
  expect(prop(group.body, 'padding-left'), 'its rule clear of the text').toBe(
    'var(--space-2)',
  )
  expect(
    prop(group.body, 'border-left'),
    '2px solid --line: decorative, the group is told by its label and by the list name from aria-labelledby, so --line needs no contrast (brief 04 criterion 4)',
  ).toBe('2px solid var(--line)')
  // The Overview list is a plain `ul` with no aria-labelledby (App.tsx), so a
  // rule that adds the group rule with the bare selector would give it one.
  const bare = TOP.filter((r) => listItems(r.sel).includes('.nav ul'))
    .filter((r) =>
      ['border-left', 'margin-left', 'padding-left'].some(
        (p) => prop(r.body, p) !== '',
      ),
    )
    .map((r) => r.flat)
  expect(
    bare,
    'the Overview list has no aria-labelledby, so the group rule must not reach it: no rule naming a bare .nav ul declares margin-left, padding-left or border-left',
  ).toEqual([])
})

// ── H5b.6: targets and reflow ───────────────────────────────────────────

test('H5b.6: a nav link is a 32 px target (2.5.8 asks 24), breaks a long instance id inside its pill (1.4.10) and is sized by min-height only, so text spacing does not clip it (1.4.12)', () => {
  const link = topBy('.nava', 'the .nav a rule')
  expect(
    Number.parseFloat(prop(link.body, 'min-height')),
    'every nav link is at least 32 px high, above the 24 px floor 2.5.8 asks',
  ).toBe(32)
  expect(
    prop(link.body, 'overflow-wrap'),
    'a long instance id breaks inside its pill, so 320 px needs no sideways scroll (1.4.10)',
  ).toBe('anywhere')
  expect(
    prop(link.body, 'height'),
    'min-height and no height, so a pill grows with the text at 200 % and text spacing clips nothing (1.4.12)',
  ).toBe('')
})
