// Hearth H2b, the containers (fe/briefs/06-hearth-h2-controls.md, section 2,
// with sections 0 and 4 as they apply to H2b): a restyle, CSS only, so these
// tests pin the stylesheet text. jsdom does no layout, so the file reads
// index.css the way DisabledLook.test.ts and HearthTokens.test.ts do (comments
// dropped, the same rule regex, the same nesting depth, prop() the last value
// of a property in a block); those files export no helper and importing a test
// file would register its tests here, so the reader is a small local copy.
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
  // combinator ("tr:last-child td"), which compact cannot.
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

// The top-level rule whose selector is exactly this, in the two spellings the
// sheet uses: the compact one (no descendant) and the flat one (a descendant
// combinator, written as the sheet writes it).
const topBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === sel.replace(/ /g, '')),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
  )

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

// Every block written with this at-rule header, as [open, end] offsets.
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

// The rule inside a block with this header whose selector is exactly `sel`.
const mediaRule = (needle: string, sel: string, what: string): Rule => {
  const hits: Rule[] = []
  for (const [open, end] of atRanges(needle))
    for (const r of RULES)
      if (
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact === sel.replace(/ /g, '')
      )
        hits.push(r)
  return single(hits, `the ${sel} rule inside ${needle} (${what})`)
}

// ── The tokens, for the contrast pairs of H2b.3 and H2b.4 ────────────────

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

// The value of one token; a miss throws naming the token, so a pair that
// lacks a token goes red on the token, not on NaN.
const valueOf = (pairs: [string, string][], name: string): string => {
  const hits = pairs.filter((p) => p[0] === name).map((p) => p[1])
  if (hits.length !== 1)
    throw new Error(`${name} is declared ${hits.length} times in the block`)
  return hits[0]
}

// WCAG 2.2 relative luminance and contrast ratio: the ratio function of
// HearthTokens.test.ts (the 0.04045 threshold), reading the hex values from
// the stylesheet.
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
const luminance = (hex: string): number => {
  const [r, g, b] = rgbOf(hex)
  return 0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b)
}
const contrast = (a: string, b: string): number => {
  const ls = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (ls[0] + 0.05) / (ls[1] + 0.05)
}
// One pair, in one theme, from the tokens written in index.css.
const pair = (theme: 'light' | 'dark', fg: string, bg: string): number => {
  const tokens = theme === 'light' ? lightTokens() : darkTokens()
  return contrast(valueOf(tokens, fg), valueOf(tokens, bg))
}

const THEMES = ['light', 'dark'] as const

// The containers H2b lifts, and the boxes whose look must not move.
const CONTAINERS = ['.card', 'table', 'th', 'td', '.save-bar', '.dialog']

// ── H2b.1: the card ───────────────────────────────────────────────────────

test('H2b.1: .card is a lifted box: --bg on --radius-card with --shadow and a 1px transparent border, padding unchanged', () => {
  const card = topBy('.card', 'the card rule')
  expect(
    prop(card.body, 'background'),
    'the card sits on --bg (H1a.8, kept)',
  ).toBe('var(--bg)')
  expect(
    prop(card.body, 'border-radius'),
    'the card corner is the card token, not the control one',
  ).toBe('var(--radius-card)')
  expect(
    prop(card.body, 'box-shadow'),
    'the lift is the shadow token, a 1px --line ring in the dark theme',
  ).toBe('var(--shadow)')
  expect(
    prop(card.body, 'border'),
    'an edge that survives forced colours, where a box-shadow computes to none (section 0, CSS Color Adjustment 1 3.1); invisible otherwise',
  ).toBe('1px solid transparent')
  expect(
    prop(card.body, 'padding'),
    'padding unchanged: the 320 px layout stays as measured',
  ).toBe('var(--space-4) var(--space-5)')
})

// ── H2b.2: the table box ─────────────────────────────────────────────────

test('H2b.2: the table is a lifted box: borders separate with no spacing, --bg on --radius-card with --shadow and a 1px transparent border, width kept', () => {
  const table = topBy('table', 'the table rule')
  expect(
    prop(table.body, 'border-collapse'),
    'border-radius has no effect on a collapsed table (section 0, CSS Backgrounds 3 4.6)',
  ).toBe('separate')
  expect(
    prop(table.body, 'border-spacing'),
    'no gaps between the cells, so the box reads as one',
  ).toBe('0')
  expect(
    prop(table.body, 'background'),
    'the table is --bg, like the card',
  ).toBe('var(--bg)')
  expect(
    prop(table.body, 'border-radius'),
    'the card corner, which is why the last row drops its divider',
  ).toBe('var(--radius-card)')
  expect(prop(table.body, 'box-shadow'), 'lifted by the shadow token').toBe(
    'var(--shadow)',
  )
  expect(
    prop(table.body, 'border'),
    'the 1px transparent edge that shows in forced colours (section 0)',
  ).toBe('1px solid transparent')
  expect(prop(table.body, 'width'), 'kept').toBe('100%')
})

// ── H2b.3: the table cells ───────────────────────────────────────────────

test('H2b.3: th is --muted at --fs-sm with the header line kept, td divides on --line, the last row drops its divider, the cell paddings and .model-profiles tr.off hold', () => {
  const th = topBy('th', 'the th rule')
  expect(
    prop(th.body, 'color'),
    'the header text is --muted on the table fill (P row: --muted on --bg)',
  ).toBe('var(--muted)')
  expect(
    prop(th.body, 'font-size'),
    'the small size of the scale, a token (H1a.7: no rem or px literal)',
  ).toBe('var(--fs-sm)')
  expect(
    prop(th.body, 'border-bottom'),
    'the header line kept, 1px solid --control (its contrast pair is the N2 one below)',
  ).toBe('1px solid var(--control)')
  const td = topBy('td', 'the td rule')
  expect(
    prop(td.body, 'border-bottom'),
    'the row divider on the decorative --line (H1a.8, kept)',
  ).toBe('1px solid var(--line)')
  const last = topBy('tr:last-child td', "the last row's divider")
  expect(
    prop(last.body, 'border-bottom'),
    'the last row drops its divider, so no straight line crosses the rounded corners (section 0)',
  ).toBe('0')
  expect(
    prop(topBy('th, td', 'the shared cell rule').body, 'padding'),
    'the cell paddings are unchanged (section 3: three columns at 320 px)',
  ).toBe('var(--space-2) var(--space-3)')
  expect(
    prop(
      topBy(
        '.model-profiles th, .model-profiles td',
        'the Model profiles cells',
      ).body,
      'padding',
    ),
    'and the Model profiles table keeps its own, unchanged',
  ).toBe('var(--space-2)')
  expect(
    prop(topBy('.model-profiles tr.off', 'the switched-off row').body, 'color'),
    'a switched-off profile stays --muted (2c.39)',
  ).toBe('var(--muted)')
})

test.each(
  THEMES.flatMap(
    (theme) =>
      [
        [theme, '--control', '--bg', 3],
        [theme, '--muted', '--bg', 4.5],
      ] as ['light' | 'dark', string, string, number][],
  ),
)(
  'H2b.3 and H2b.4: %s on %s clears the %s:1 floor in %s',
  (theme, fg, bg, floor) => {
    const ratio = pair(theme, fg, bg)
    expect(
      ratio,
      `${fg} on ${bg} in ${theme}: ${ratio.toFixed(2)}:1, floor ${floor}:1 (the header line and the bar edge on the box fill, 1.4.11; the header text, 1.4.3)`,
    ).toBeGreaterThanOrEqual(floor)
  },
)

test('H2b.3 (N2): the header line and the bar edge measure the 3.91 light and 4.12 dark the brief states (--control on --bg)', () => {
  for (const [theme, want] of [
    ['light', '3.91'],
    ['dark', '4.12'],
  ] as ['light' | 'dark', string][])
    expect(
      pair(theme, '--control', '--bg').toFixed(2),
      `--control on --bg in ${theme}, the brief's N2 measurement ${want}`,
    ).toBe(want)
})

// ── H2b.4: the save bar ──────────────────────────────────────────────────

test('H2b.4: the save bar is a lifted box with the card radius on its top corners, and its sticky position, bottom, z-index, paddings and short-viewport rule are unchanged', () => {
  const bar = topBy('.save-bar', 'the save bar rule')
  expect(
    prop(bar.body, 'background'),
    'the bar is --bg, the same fill as the card it sits over (H1a.8, kept)',
  ).toBe('var(--bg)')
  expect(
    prop(bar.body, 'border-top'),
    'the bar edge over that fill stays the coloured 1px --control line (N2), which is what shows in forced colours',
  ).toBe('1px solid var(--control)')
  expect(
    prop(bar.body, 'border-radius'),
    'the card corner on the two top corners only: the bar is flush at the bottom',
  ).toBe('var(--radius-card) var(--radius-card) 0 0')
  expect(prop(bar.body, 'box-shadow'), 'lifted by the shadow token').toBe(
    'var(--shadow)',
  )
  // The focus-not-obscured machinery of 2h.135 is unchanged; the 7rem floor of
  // the scroll padding is pinned in DisabledLook.test.ts.
  expect(prop(bar.body, 'position'), 'the bar stays sticky').toBe('sticky')
  expect(prop(bar.body, 'bottom'), 'at the bottom edge').toBe('0')
  expect(prop(bar.body, 'z-index'), 'above the cards').toBe('1')
  expect(prop(bar.body, 'padding'), 'paddings unchanged').toBe(
    'var(--space-3) var(--space-4)',
  )
  expect(
    prop(
      mediaRule(
        '@media (max-height: 31.98rem)',
        '.save-bar',
        'the short viewport rule (2h.135)',
      ).body,
      'position',
    ),
    'on a short viewport the bar is still not sticky',
  ).toBe('static')
})

// ── H2b.5: the dialog ────────────────────────────────────────────────────

test('H2b.5: the dialog takes the card corner and the shadow, keeps its --bg fill and its 1px --control border, and ::backdrop is unchanged', () => {
  const dialog = topBy('.dialog', 'the dialog rule')
  expect(
    prop(dialog.body, 'border-radius'),
    'the card corner, like the card and the table',
  ).toBe('var(--radius-card)')
  expect(prop(dialog.body, 'box-shadow'), 'lifted by the shadow token').toBe(
    'var(--shadow)',
  )
  expect(prop(dialog.body, 'background'), 'the --bg fill is kept').toBe(
    'var(--bg)',
  )
  expect(
    prop(dialog.body, 'border'),
    'the coloured border is kept: a border survives forced colours where a fill does not (section 0), so the dialog needs no transparent one',
  ).toBe('1px solid var(--control)')
  expect(
    prop(topBy('.dialog::backdrop', 'the dialog backdrop').body, 'background'),
    '::backdrop unchanged (H1a.4 allows only this colour literal outside the token blocks)',
  ).toBe('rgb(0 0 0 / 0.5)')
})

// ── Section 0: the lifted boxes in forced colours ────────────────────────

test('section 0: the boxes lifted by a shadow alone carry the 1px transparent border, the boxes with a coloured border do not, and H2b adds no forced-colours block', () => {
  for (const sel of ['.card', 'table'])
    expect(
      prop(topBy(sel, `the ${sel} rule`).body, 'border'),
      `${sel}: its only other edge is the shadow, which computes to none in forced colours, so the border is what keeps the box an edge there`,
    ).toBe('1px solid transparent')
  expect(
    topBy('.save-bar', 'the save bar rule').body.includes(
      'border: 1px solid transparent',
    ),
    'the bar has a coloured top border, which shows there: no transparent border',
  ).toBe(false)
  expect(
    topBy('.dialog', 'the dialog rule').body.includes(
      'border: 1px solid transparent',
    ),
    'the dialog keeps its --control border: no transparent border',
  ).toBe(false)
  expect(
    atRanges('@media (forced-colors: active)').length,
    'the forced-colours blocks stay the five of H1a, H2a.6 and H6a.10: H2b adds none, it relies on the borders (section 4)',
  ).toBe(5)
})

// ── H2b.6: nothing else moves ────────────────────────────────────────────

test('H2b.6: the notice and the alert keep their border and --radius, the banner and the nav sit on the paper (H5a.2, H5b.1), only the four containers take --shadow, and no container gains a state rule', () => {
  for (const [sel, border] of [
    ['.notice', '1px solid var(--control)'],
    ['.alert', '1px solid var(--danger)'],
  ] as [string, string][]) {
    const rule = topBy(sel, `the ${sel} rule`)
    expect(
      prop(rule.body, 'border'),
      `${sel} keeps its border: a notice is a bordered paragraph, and a border survives forced colours where a fill does not (section 0: no H2 change)`,
    ).toBe(border)
    expect(
      prop(rule.body, 'border-radius'),
      `${sel} keeps --radius: the card corner is for the lifted boxes (section 0)`,
    ).toBe('var(--radius)')
    expect(prop(rule.body, 'box-shadow'), `${sel} is not lifted`).toBe('')
  }
  // H5a.2 (07-hearth-h5-shell.md) restyled the banner: it sits on the paper,
  // so --surface and the divider are gone. H5b.1 gave the nav the same.
  const banner = topBy('.banner', 'the .banner rule')
  expect(
    prop(banner.body, 'background'),
    'the banner is transparent: the shell sits on the page (H5a.2)',
  ).toBe('transparent')
  expect(
    ['', '0'].includes(prop(banner.body, 'border-bottom')),
    'the banner has no border-bottom declaration, or border-bottom: 0 (H5a.2)',
  ).toBe(true)
  expect(prop(banner.body, 'box-shadow'), '.banner is not lifted').toBe('')
  const nav = topBy('.nav', 'the .nav rule')
  expect(
    prop(nav.body, 'background'),
    'the nav is transparent too: the shell sits on the paper, the cards lift off it (H5b.1)',
  ).toBe('transparent')
  expect(
    prop(nav.body, 'border-bottom'),
    'the nav bottom edge is the decorative --line, seen only below 960 px when the menu is open (H5b.1)',
  ).toBe('1px solid var(--line)')
  expect(prop(nav.body, 'box-shadow'), '.nav is not lifted').toBe('')
  const lifted = RULES.filter(
    (r) => prop(r.body, 'box-shadow') === 'var(--shadow)',
  )
    .map((r) => r.flat)
    .sort()
  expect(
    lifted,
    'the boxes drawn with the shadow token are exactly the containers of H2b (.card, table, .save-bar, .dialog) plus the tile H6a.6 lifts with the same token',
  ).toEqual(['.card', '.dialog', '.save-bar', '.tile', 'table'].sort())
  // A container carries no state look: H2 adds no hover, active, focus,
  // disabled, checked or aria rule on the containers (section 0: no hover look
  // on controls, and H2b adds none on a box). A rule on a child (.card h3,
  // .card .note) or a pseudo-element (.dialog::backdrop) is not a state of the
  // container, so the remainder of the selector must hold no space and no `::`.
  const stateful = RULES.filter((r) =>
    CONTAINERS.some((c) => {
      const rest = r.flat.startsWith(c) ? r.flat.slice(c.length) : null
      return (
        rest !== null &&
        !rest.includes(' ') &&
        !rest.includes('::') &&
        /[:[]/.test(rest)
      )
    }),
  ).map((r) => r.flat)
  expect(stateful, 'no state rule on a container').toEqual([])
  expect(
    RULES.filter((r) => r.flat.includes(':hover')).map((r) => r.flat),
    'the sheet keeps the nav link of 2h and the id link H6a.7 underlines under the pointer, and nothing else: H2b adds no hover look (section 0)',
  ).toEqual(['.nav a:hover', '.tile-id:hover'])
})

// ── Section 4: text spacing ──────────────────────────────────────────────

test('section 4 (1.4.12): no container that holds text takes a fixed height: the height of a box with text is a min-height, and H2b sets none', () => {
  const fixed: string[] = []
  for (const sel of CONTAINERS) {
    const height = prop(topBy(sel, `the ${sel} rule`).body, 'height')
    if (height) fixed.push(`${sel}: height: ${height}`)
  }
  expect(
    fixed,
    'a height clips the text a card, a table cell, the bar or the dialog holds when the user widens the text; the containers are sized by their content',
  ).toEqual([])
})
