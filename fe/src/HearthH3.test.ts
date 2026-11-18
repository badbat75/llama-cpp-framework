// Hearth H3: the mark in the banner and the favicon. Brief
// fe/briefs/07-hearth-h3-mark.md, sections 3.4 to 3.8 and 3.10.
//
// The geometry of the mark (the shape, the 2rem, the gap) is the stylesheet's,
// and jsdom lays nothing out: these tests read the stylesheet's text and
// index.html's text, the way HearthH2a.test.ts and HearthTokens.test.ts read
// them. The dark block of H1a.2 stays the locator.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'

const CSS = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
const CSS_NOCOMMENTS = CSS.replace(/\/\*[\s\S]*?\*\//g, '')
const HTML = readFileSync(join(import.meta.dirname, '..', 'index.html'), 'utf8')

/** The outline path of the hearth mark, the one the banner and the favicon share. */
const D =
  'M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z'

/** The declarations of the top-level rule listing `selector`; throws when the
 *  rule is missing, which is the red for a behaviour that is not there yet. */
function rule(selector: string): string {
  for (const m of CSS_NOCOMMENTS.matchAll(/([^{}]+)\{/g)) {
    const start = m.index!
    if (atRanges().some(([s, e]) => start >= s && start < e)) continue
    if (
      !m[1]!
        .split(',')
        .map((s) => s.trim())
        .includes(selector)
    )
      continue
    const rest = CSS_NOCOMMENTS.slice(start + m[0].length)
    const end = rest.indexOf('}')
    if (end < 0) continue
    return rest.slice(0, end)
  }
  throw new Error(`index.css has no top-level "${selector}" rule`)
}

/** The text ranges of the `@media` blocks, so the top-level rules can skip them. */
function atRanges(): [number, number][] {
  const out: [number, number][] = []
  for (const m of CSS_NOCOMMENTS.matchAll(/@media[^{]*\{/g)) {
    let depth = 1
    let i = m.index! + m[0].length
    while (depth > 0 && i < CSS_NOCOMMENTS.length) {
      const c = CSS_NOCOMMENTS[i]
      if (c === '{') depth++
      else if (c === '}') depth--
      i++
    }
    out.push([m.index!, i])
  }
  return out
}

/** The text of every `@media (forced-colors: active)` block, braces included. */
function forcedBlocks(): string[] {
  const out: string[] = []
  for (const [start, end] of atRanges()) {
    const header = CSS_NOCOMMENTS.slice(
      start,
      CSS_NOCOMMENTS.indexOf('{', start),
    )
      .replace(/\s+/g, ' ')
      .trim()
    if (header === '@media (forced-colors: active)')
      out.push(CSS_NOCOMMENTS.slice(start, end))
  }
  return out
}

/** Every rule body inside a forced-colours block whose selector list is
 *  exactly `selector` (a selector of the list matched whole, not inside). */
function forcedRule(selector: string): string[] {
  const hits: string[] = []
  for (const block of forcedBlocks())
    for (const m of block.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const list = m[1]!.split(',').map((s) => s.trim())
      if (list.includes(selector)) hits.push(m[2]!)
    }
  return hits
}

/** One declaration's value inside a rule body, or `undefined` when the rule leaves it out. */
function prop(body: string, name: string): string | undefined {
  const re = new RegExp('(?:^|;)[ \\t\\n]*' + name + '[ \\t]*:[ \\t]*([^;]*)')
  return body.match(re)?.[1]?.trim()
}

/** Every custom property declared in `block`, in order. */
function customProps(block: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const m of block.matchAll(/(--[a-zA-Z-]+)\s*:\s*([^;]*);/g))
    out[m[1]!] = m[2]!.trim()
  return out
}

/** The light `:root` block (the file's first). */
function lightTokens(): Record<string, string> {
  const start = CSS_NOCOMMENTS.indexOf(':root')
  const end = CSS_NOCOMMENTS.indexOf('}', start)
  return customProps(CSS_NOCOMMENTS.slice(start, end))
}

/** The `:root` block of the dark scheme block (H1a.2). */
function darkTokens(): Record<string, string> {
  const media = CSS_NOCOMMENTS.indexOf('@media (prefers-color-scheme: dark)')
  const start = CSS_NOCOMMENTS.indexOf(':root', media)
  const end = CSS_NOCOMMENTS.indexOf('}', start)
  return customProps(CSS_NOCOMMENTS.slice(media, end))
}

/** WCAG contrast ratio of two `#rrggbb` colours. */
function ratio(a: string, b: string): number {
  const lum = (h: string) => {
    const c = [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
    const l = c.map((v) =>
      v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4),
    )
    return 0.2126 * l[0]! + 0.7152 * l[1]! + 0.0722 * l[2]!
  }
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}

/** The favicon's href, as index.html writes it (the value carries single
 *  quotes, so the attribute is read between its own double quotes). */
function iconHref(): string {
  const link = HTML.match(/<link[^>]*rel=["']icon["'][^>]*>/)?.[0]
  if (link === undefined) throw new Error('index.html has no <link rel="icon">')
  const href =
    link.match(/href="([^"]*)"/)?.[1] ?? link.match(/href='([^']*)'/)?.[1]
  if (href === undefined) throw new Error('the icon link has no href')
  return href
}

// ── H3.4: the name and the mark in one line ─────────────────────────────

test('H3.4: .app-name lays the mark and the name in one line, centred, gapped by --space-2', () => {
  const b = rule('.app-name')
  // The brief 3.4: the new lines, which H1a.3 never had.
  expect(
    prop(b, 'display'),
    'inline-flex, so the mark sits on the text line',
  ).toBe('inline-flex')
  expect(prop(b, 'align-items'), 'the mark centred on the cap height').toBe(
    'center',
  )
  expect(prop(b, 'gap'), 'one token, no literal rem').toBe('var(--space-2)')
  // What H1a.3 pinned stays.
  expect(prop(b, 'color')).toBe('var(--text)')
  expect(prop(b, 'font-weight')).toBe('600')
  expect(prop(b, 'text-decoration')).toBe('none')
  expect(prop(b, 'padding')).toBe('var(--space-1) 0')
})

// ── H3.5: the size, the flex and the colour of the mark ─────────────────

test('H3.5: .mark is 2rem, never shrinks, and takes the accent through currentColor', () => {
  const b = rule('.mark')
  expect(prop(b, 'width')).toBe('2rem')
  expect(prop(b, 'height')).toBe('2rem')
  expect(prop(b, 'flex'), 'never shrinks, as .icon does').toBe('none')
  expect(prop(b, 'color')).toBe('var(--accent)')
  // The brief 3.5: `fill: currentColor` on the path is right, a `fill` here is
  // the one declaration that would break the accent in the favicon's stead.
  expect(
    prop(b, 'fill'),
    'no fill: the currentColor is the path business',
  ).toBeUndefined()
})

const accentContrast: [string, number, number][] = [
  // light: 6.46 on --surface, 6.58 on --page; dark: 8.04 and 9.37.
  ['light', 6.46, 6.58],
  ['dark', 8.04, 9.37],
]
test.each(accentContrast)(
  'H3.5: --accent against both page colours in the %s scheme clears AA large (3:1)',
  (_scheme, onSurface, onPage) => {
    const tok = _scheme === 'light' ? lightTokens() : darkTokens()
    const accent = tok['--accent']!
    const surface = ratio(accent, tok['--surface']!)
    const page = ratio(accent, tok['--page']!)
    expect(surface.toFixed(2)).toBe(onSurface.toFixed(2))
    expect(page.toFixed(2)).toBe(onPage.toFixed(2))
    expect(surface).toBeGreaterThanOrEqual(3)
    expect(page).toBeGreaterThanOrEqual(3)
  },
)

// ── H3.7: the favicon ───────────────────────────────────────────────────

test('H3.7: the favicon is the same hearth mark, in the accent of each scheme', () => {
  const href = iconHref()
  expect(href.startsWith('data:image/svg+xml,')).toBe(true)
  const svg = decodeURIComponent(href.slice('data:image/svg+xml,'.length))
  // The shape: one path, the outline path and the even-odd rule of the brief.
  expect(svg.match(/<path/g)?.length).toBe(1)
  expect(svg).toContain(`d='${D}'`)
  expect(svg).toContain("fill-rule='evenodd'")
  expect(svg).toContain("viewBox='2 3 28 28'")
  // The colour: both schemes, in the style block, as the brief writes it.
  expect(svg).toContain('@media (prefers-color-scheme:dark)')
  expect(
    svg,
    'the light fill, then the dark one inside the media query',
  ).toContain(
    `@media (prefers-color-scheme:dark){path{fill:${darkTokens()['--accent']}}}`,
  )
})

test('H3.7: the favicon carries exactly the two accents, light then dark, and nothing else', () => {
  const href = iconHref()
  const hexes = href.match(/%23[0-9a-fA-F]{6}/g) ?? []
  expect(hexes.length, 'two six-digit hexes, no more').toBe(2)
  // The href encodes the `#` as `%23` (slice(3) drops it), the token carries it.
  expect('#' + hexes[0]!.slice(3).toLowerCase()).toBe(
    lightTokens()['--accent']!.toLowerCase(),
  )
  expect('#' + hexes[1]!.slice(3).toLowerCase()).toBe(
    darkTokens()['--accent']!.toLowerCase(),
  )
  // No other way to write a colour in it.
  expect(href, 'a raw # is not a valid data URI colour').not.toContain('#')
  const svg = decodeURIComponent(href.slice('data:image/svg+xml,'.length))
  expect(svg.match(/#[0-9a-fA-F]{3,8}\b/g)?.length).toBe(2)
  expect(svg).not.toMatch(/\b(rgb|hsl)a?\(/)
  expect(svg, 'no colour word beside the two accents').not.toMatch(
    /\b(white|black|red|blue|green|gray|grey|yellow|purple|orange|pink|brown)\b/,
  )
})

// ── H3.8: the mark carries no colour literal ────────────────────────────

test('H3.8: the mark rule carries no colour literal, only tokens', () => {
  const b = rule('.mark')
  expect(b, 'no hex in the mark rule').not.toMatch(/#[0-9a-fA-F]{3,8}\b/)
  expect(b, 'no functional colour in the mark rule').not.toMatch(
    /\b(rgb|hsl)a?\(/,
  )
  expect(b, 'no named colour in the mark rule').not.toMatch(
    /:\s*(white|black|red|blue|green|gray|grey|currentColor)\b/,
  )
  // The accent reaches the mark as a token, which is what makes H1a.2's dark
  // scheme switch it with no mark change (the brief 3.8).
  expect(prop(b, 'color')).toBe('var(--accent)')
})

// ── H3.10: the mark in forced colours ───────────────────────────────────

test('H3.10: in forced colours the mark takes the link colour: .mark { color: inherit } in a forced-colours block', () => {
  // The brief 0 (corrected after the review) and 3.10: the UA sheet gives an
  // `svg` `forced-color-adjust: preserve-parent-color`, so the author accent
  // survives the forced colours and the mark keeps its plum while the link
  // takes LinkText. One `color: inherit` inside a forced-colours block hands
  // the mark the link's system colour.
  const hits = forcedRule('.mark')
  expect(
    hits.length,
    'one .mark rule inside the @media (forced-colors: active) blocks',
  ).toBe(1)
  const b = hits[0]!
  expect(prop(b, 'color'), 'the link colour, inherited: not the accent').toBe(
    'inherit',
  )
  // The brief 3.10: "no other declaration". The size, the flex and the rest
  // stay the top-level rule's, and a forced-color-adjust here is exactly the
  // spelling that keeps the accent on the mark.
  const decls = b
    .split(';')
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .filter((d) => d !== '')
  expect(
    decls,
    'the rule adds the one declaration and nothing beside it',
  ).toEqual(['color'])
  // H3.5 stands in the forced block too: no fill there.
  expect(
    prop(b, 'fill'),
    'no fill: the path keeps currentColor',
  ).toBeUndefined()
})
