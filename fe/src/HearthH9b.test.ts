// Hearth H9b, the model profile page's jump list as a sticky rail from 1280 px
// (fe/briefs/08-hearth-h9-jump.md, section 2): the stylesheet plane. H9b is CSS
// only (section 2, "DOM: none"), so these tests read index.css as text the way
// HearthH9a.test.ts and HearthH8a.test.ts read them (comments dropped, rules
// from the brace balance, nesting from the balance before the selector); a test
// file imports no other test file, so the readers are local copies.
//
// H9b.1 and H9b.2 are pinned as declarations inside the one
// @media (min-width: 1280px) block, found by matched braces; H9b.3 is pinned as
// "no rule outside that block names section:has(> .jump)", vacuously true today
// (no rail rule exists yet): that one is a guard, green from day one.
//
// The cascade is pinned as its own scan, in both directions:
//  - railLoses: among the rules that can apply 1280 px wide (the top level, the
//    min-width blocks, the max-height and forced-colours blocks; the
//    max-width: 959.98 px and 479.98 px blocks cannot, their windows end below
//    1280), no rule of a higher specificity, or of the same one written after,
//    beats the rail's display, grid-template-columns, position, top or
//    max-height on the section, the rail nav or the save bar.
//  - railWinsOnShort: the rail never beats a rule of the short-viewport block
//    (max-height: 31.98rem) that sets the same property on the same element:
//    that block applies at 1280 px wide on a short viewport and must keep
//    winning (the .save-bar { position: static } of H5c.2 is its case). The two
//    max-width blocks need no scan: a min-width: 1280px rule never applies in
//    their windows, so they cannot lose to the rail.
//  Both scans carry self-probes over sheets written here: the scans are what
//  give the green its meaning. Specificities count in three parts, and :has()
//  takes the specificity of its argument (CSS Selectors 4, 7.2.10), the spelling
//  the brief's "(0,1,1)" for .jump button counts by.
//
// What the text cannot pin goes to the designer's UX pass (section 3): the rail
// beside the cards at 1280, sticky to the last card, its entries not under the
// save bar (H9b.5's geometry, 2.4.11 measured by tabbing at 1280). The DOM
// halves of H9b.4, H9b.5 and H9b.6 are in HearthJumpRail.test.tsx.
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

// The rules of a CSS text, with the nesting level of the rule that starts at
// each offset (0 at the top level). Comments become spaces, which keeps the
// offsets of the text the scan was given. RULES is the sheet's.
const rulesOf = (css: string): Rule[] => {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, (m) => ' '.repeat(m.length))
  const opened = (upto: number): number =>
    (stripped.slice(0, upto).match(/\{/g) ?? []).length -
    (stripped.slice(0, upto).match(/\}/g) ?? []).length
  return [...stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    sel: m[1].trim(),
    // Whitespace dropped: a needle can name a descendant with its space gone.
    compact: m[1].replace(/\s+/g, ''),
    // Runs of whitespace become one space: how the sheet writes a descendant.
    flat: m[1].trim().replace(/\s+/g, ' '),
    body: m[2],
    at: m.index ?? 0,
    depth: opened(m.index ?? 0),
  }))
}

const RULES: Rule[] = rulesOf(CSS)

// The value of a declaration; the last one when a block repeats the property.
// Runs of whitespace collapse to one space: Prettier re-wraps a long value and
// an exact compare then reds on the wrap, not on the code.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .trim()
        .replace(/\s+/g, ' ')
  }
  return val
}

// The properties a block declares, sorted: a set equality is what refuses a
// property the brief does not list.
const decls = (body: string): string[] =>
  body
    .split(';')
    .filter((d) => d.includes(':'))
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .sort()

// A throw names the miss, so "the rule is missing" never reads as a typo.
const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length ? ` (${list.map((r) => r.flat).join(' | ')})` : ''),
    )
  return list[0]
}

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
// closing brace; and every block of a CSS text written with this header.
const rangeEndIn = (css: string, open: number): number => {
  let end = open
  let depth = 0
  for (; end < css.length; end++) {
    if (css[end] === '{') depth++
    else if (css[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

const atRangesIn = (css: string, needle: string): [number, number][] => {
  const ranges: [number, number][] = []
  let from = 0
  for (;;) {
    const at = css.indexOf(needle, from)
    if (at < 0) return ranges
    const end = rangeEndIn(css, at)
    ranges.push([at, end])
    from = end + 1
  }
}

// The name a rule shows in a message: its selector, whitespace collapsed.
const where = (sel: string): string => sel.replace(/\s+/g, ' ').trim()

// The three selectors of the rail, spelled as the brief gives them.
const RAIL1280 = '@media (min-width: 1280px)'
const SHORT = '@media (max-height: 31.98rem)'
const SEC_SEL = 'section:has(> .jump)'
const KID_SEL = 'section:has(> .jump) > *'
const RAIL_SEL = 'section:has(> .jump) > .jump'

// The one rail block of a CSS text; a miss names the block, so the red of a
// sheet without it reads as "the block is missing", not as a broken fixture.
const railRanges = (css: string): [number, number] => {
  const ranges = atRangesIn(css, RAIL1280)
  if (ranges.length !== 1)
    throw new Error(
      `expected exactly one ${RAIL1280} block (H9b.1 places the rail rules in it), found ${ranges.length}`,
    )
  return ranges[0]
}

const railRulesIn = (css: string): Rule[] => {
  const [open, end] = railRanges(css)
  return rulesOf(css).filter((r) => r.depth > 0 && r.at > open && r.at < end)
}

// A rule of the rail block whose selector list is exactly this selector.
const railBy = (css: string, sel: string, what: string): Rule =>
  single(
    railRulesIn(css).filter(
      (r) => listItems(r.sel).length === 1 && listItems(r.sel)[0] === sel,
    ),
    `the rule "${sel}" inside the ${RAIL1280} block (${what})`,
  )

// ── H9b.1: the section becomes a two-column grid ───────────────────────────

test('H9b.1: inside @media (min-width: 1280px), section:has(> .jump) declares the grid, the 15rem rail column, the --space-6 column gap, items at start, and the max-width that outgrows main > *: 880px plus the rail', () => {
  const rule = railBy(CSS, SEC_SEL, 'the rail grid')
  const WANT: [string, string][] = [
    ['display', 'grid'],
    ['grid-template-columns', 'minmax(0, 1fr) 15rem'],
    ['column-gap', 'var(--space-6)'],
    ['align-items', 'start'],
    ['max-width', 'calc(880px + 15rem + var(--space-6))'],
  ]
  for (const [name, want] of WANT)
    expect(
      prop(rule.body, name),
      `section:has(> .jump) ${name}: H9b.1 gives ${want}`,
    ).toBe(want)
  expect(
    decls(rule.body),
    'H9b.1 lists these properties and no other on the section',
  ).toEqual(WANT.map(([n]) => n).sort())
  // The max-width is a cascade fact: main > * caps every page child at 880 px
  // and the rail section must outgrow it at 1280 px and above.
  const cap = single(
    RULES.filter((r) => r.depth === 0 && r.compact === 'main>*'),
    'the main > * rule (the 880 px cap)',
  )
  expect(prop(cap.body, 'max-width'), 'main > * caps at 880px').toBe('880px')
  expect(
    specGt(specOf(SEC_SEL), specOf('main > *')),
    '0,1,1 over 0,0,1: the rail rule outgrows the cap by its own specificity, in the block and at any order',
  ).toBe(true)
})

// ── H9b.2: the placement ───────────────────────────────────────────────────

test('H9b.2: section:has(> .jump) > * declares grid-column: 1 and nothing else, so the h2, the notice, the Engine field, the cards and the bar all keep the first column', () => {
  const rule = railBy(CSS, KID_SEL, 'the first column for every child')
  expect(
    prop(rule.body, 'grid-column'),
    'every child of the section sits in column 1: the bar spans the content column, not the page (H9b.5)',
  ).toBe('1')
  expect(
    decls(rule.body),
    'this one declaration and no other: a property here reaches every child of the page, so an extra one is a page-wide change (H9b.6: nothing else moves)',
  ).toEqual(['grid-column'])
})

test('H9b.2: section:has(> .jump) > .jump declares the rail: column 2, row 2 spanning 40, sticky at --space-4, a 100vh minus --space-4 max-height, auto scroll, no margin, inline padding --space-2', () => {
  const rule = railBy(CSS, RAIL_SEL, 'the sticky rail')
  const WANT: [string, string][] = [
    ['grid-column', '2'],
    ['grid-row', '2 / span 40'],
    ['position', 'sticky'],
    ['top', 'var(--space-4)'],
    ['max-height', 'calc(100vh - 2 * var(--space-4))'],
    ['overflow-y', 'auto'],
    ['margin', '0'],
    // overflow-y: auto makes overflow-x compute to auto too (CSS Overflow 3),
    // so the rail clips what is drawn outside its box: the entries' ring at
    // 2 px offset lies wholly outside on the left and the right, and 8 px of
    // inline padding on each side keeps it whole.
    ['padding-inline', 'var(--space-2)'],
  ]
  for (const [name, want] of WANT)
    expect(
      prop(rule.body, name),
      `section:has(> .jump) > .jump ${name}: H9b.2 gives ${want}`,
    ).toBe(want)
  expect(
    decls(rule.body),
    'H9b.2 lists these properties and no other on the rail',
  ).toEqual(WANT.map(([n]) => n).sort())
})

// ── H9b.3: below 1280 px nothing of H9b applies ────────────────────────────

test('H9b.3: no rule outside the min-width: 1280px block names section:has(> .jump), so below 1280 px the list stays where H9a draws it (guard: vacuously true until a rail rule exists)', () => {
  const ranges = atRangesIn(CSS, RAIL1280)
  const outside = RULES.filter(
    (r) =>
      r.compact.includes('section:has(>.jump)') &&
      !ranges.some(([open, end]) => r.depth > 0 && r.at > open && r.at < end),
  )
  expect(
    outside.map((r) => `${where(r.sel)} at depth ${r.depth}`),
    'a rail selector written outside the block would restyle the page at every width: H9b.3 says the rail exists at 1280 px and above only',
  ).toEqual([])
})

// ── The cascade of the rail (REMINDER b): specificity and source order ─────

// A specificity in three parts: ids, then classes/attributes/pseudo-classes,
// then elements/pseudo-elements. A :has() takes the specificity of its
// argument, the largest of its selector list (CSS Selectors 4, 7.2.10): the
// brief counts `.jump button` as (0,1,1), and `section:has(> .jump)` must come
// out at (0,1,1), not (0,2,1), for the two counts to mean the same thing.
type Spec = [number, number, number]

const TOKEN =
  /\[[^\]]*\]|::[a-zA-Z-]+|:[a-zA-Z-]+(?:\([^)]*\))?|\.[a-zA-Z-]+|#[a-zA-Z-]+|[a-zA-Z][a-zA-Z0-9-]*|[*+>~]|\s+/g

const specOf = (raw: string): Spec => {
  const s: Spec = [0, 0, 0]
  const max = (arg: string): Spec => {
    let best: Spec = [0, 0, 0]
    for (const part of listItems(arg)) {
      const p = specOf(part)
      if (specGt(p, best)) best = p
    }
    return best
  }
  for (const m of raw.trim().matchAll(TOKEN)) {
    const t = m[0]
    if (/^\s/.test(t) || t === '*' || t === '+' || t === '>' || t === '~')
      continue
    const fn = /^:has\((.*)\)$/.exec(t)
    if (fn) {
      const a = max(fn[1])
      s[0] += a[0]
      s[1] += a[1]
      s[2] += a[2]
      continue
    }
    if (t[0] === '#') s[0]++
    else if (t.startsWith('::')) s[2]++
    else if (t[0] === '.' || t[0] === '[' || t[0] === ':') s[1]++
    else s[2]++
  }
  return s
}

const specGt = (a: Spec, b: Spec): boolean =>
  a[0] !== b[0] ? a[0] > b[0] : a[1] !== b[1] ? a[1] > b[1] : a[2] > b[2]

const specEq = (a: Spec, b: Spec): boolean =>
  a[0] === b[0] && a[1] === b[1] && a[2] === b[2]

// The elements a rail rule styles, as App.tsx and ModelProfilePage.tsx render
// them, from the element outwards (the chain of HearthH8a's Menu scan).
type El = { tag: string; id: string; classes: string[] }

const SECTION: El = { tag: 'section', id: '', classes: [] }
const MAIN: El = { tag: 'main', id: 'main', classes: [] }
const LAYOUT: El = { tag: 'div', id: '', classes: ['layout'] }
const ROOT: El = { tag: 'div', id: 'root', classes: [] }
const BODY: El = { tag: 'body', id: '', classes: [] }
const HTML: El = { tag: 'html', id: '', classes: [] }

const CHAINS: { name: string; els: El[] }[] = [
  { name: 'the page section', els: [SECTION, MAIN, LAYOUT, ROOT, BODY, HTML] },
  {
    name: 'the rail (nav.jump)',
    els: [
      { tag: 'nav', id: '', classes: ['jump'] },
      SECTION,
      MAIN,
      LAYOUT,
      ROOT,
      BODY,
      HTML,
    ],
  },
  {
    name: 'the save bar',
    els: [
      { tag: 'div', id: '', classes: ['save-bar'] },
      SECTION,
      MAIN,
      LAYOUT,
      ROOT,
      BODY,
      HTML,
    ],
  },
]

// The properties the rail rules of H9b.1 and H9b.2 set and the scan weighs:
// exactly the five the cascade pass names.
const RAIL_PROPS = [
  'display',
  'grid-template-columns',
  'position',
  'top',
  'max-height',
]

// A selector part split into its compounds at the whitespace and the
// combinators OUTSIDE brackets and parentheses (the space inside
// `:has(> .jump)` is not a combinator). Each compound carries the combinator
// before it.
const chainOf = (raw: string): { comb: string; text: string }[] => {
  const out: { comb: string; text: string }[] = []
  let cur = ''
  let comb = ' '
  let depth = 0
  const push = (): void => {
    if (cur === '') return
    out.push({ comb, text: cur })
    cur = ''
    comb = ' '
  }
  for (const ch of raw.trim()) {
    if (ch === '(' || ch === '[') depth++
    else if (ch === ')' || ch === ']') depth--
    if (depth > 0 || !/[\s>+~]/.test(ch)) {
      cur += ch
      continue
    }
    if (/\s/.test(ch)) push()
    else {
      push()
      comb = ch
    }
  }
  push()
  return out
}

// One compound: its tag, its id, its classes. An attribute selector or a
// pseudo-class is a constraint this test cannot evaluate, so it is ignored: a
// rule carrying one is scanned, not skipped.
const compoundOf = (text: string): El => {
  const out = { tag: '', id: '', classes: [] as string[] }
  for (const m of text.matchAll(TOKEN)) {
    const t = m[0]
    if (t[0] === '#') out.id = t.slice(1)
    else if (t[0] === '.') out.classes.push(t.slice(1))
    else if (t[0] === '[' || t[0] === ':') continue
    else if (t !== '*') out.tag = t
  }
  return out
}

const fits = (c: El, el: El): boolean =>
  (c.tag === '' || c.tag === el.tag) &&
  (c.id === '' || c.id === el.id) &&
  c.classes.every((k) => el.classes.includes(k))

// Does this selector part reach this chain's element? Its LAST compound fits
// the element; the compounds before it are walked up the chain: a descendant
// may be any ancestor, a child must be the one, a sibling combinator never
// reaches a first child.
const matchesChain = (els: El[], raw: string): boolean => {
  const chain = chainOf(raw)
  if (chain.length === 0) return false
  if (!fits(compoundOf(chain[chain.length - 1].text), els[0])) return false
  let i = 1
  for (let k = chain.length - 2; k >= 0; k--) {
    const c = compoundOf(chain[k].text)
    const comb = chain[k + 1].comb
    if (comb === '+' || comb === '~') return false
    if (comb === '>') {
      if (i >= els.length || !fits(c, els[i])) return false
      i++
      continue
    }
    while (i < els.length && !fits(c, els[i])) i++
    if (i >= els.length) return false
    i++
  }
  return true
}

// The highest specificity among the parts of a selector list that reach the
// chain; null when none does (the specificity of the part that matches, not of
// the loudest part the list also carries).
const chainSpec = (sel: string, els: El[]): Spec | null => {
  let best: Spec | null = null
  for (const part of listItems(sel)) {
    if (!matchesChain(els, part)) continue
    const s = specOf(part)
    if (best === null || specGt(s, best)) best = s
  }
  return best
}

// The rail rule of a chain and property with the highest claim: highest
// specificity, and at a tie the last written (the cascade's own order).
const railClaim = (
  rules: Rule[],
  els: El[],
  p: string,
): { rule: Rule; spec: Spec } | null => {
  let best: { rule: Rule; spec: Spec } | null = null
  for (const r of rules) {
    if (prop(r.body, p) === '') continue
    const s = chainSpec(r.sel, els)
    if (s === null) continue
    if (
      best === null ||
      specGt(s, best.spec) ||
      (specEq(s, best.spec) && r.at > best.rule.at)
    )
      best = { rule: r, spec: s }
  }
  return best
}

// Every rule that can apply 1280 px wide (top level, and inside blocks whose
// window reaches 1280: the min-width, max-height and forced-colours blocks; a
// max-width block under 1280 cannot) and beats the rail's rule for a property
// on the same element: a higher specificity, or the same one written after.
const railLoses = (css: string): string[] => {
  const [open, end] = railRanges(css)
  const rules = rulesOf(css)
  const rail = rules.filter((r) => r.depth > 0 && r.at > open && r.at < end)
  const below1280 = (r: Rule): boolean =>
    r.depth > 0 &&
    ['@media (max-width: 959.98px)', '@media (max-width: 479.98px)'].some((n) =>
      atRangesIn(css, n).some(([a, e]) => r.at > a && r.at < e),
    )
  const out: string[] = []
  for (const chain of CHAINS)
    for (const p of RAIL_PROPS) {
      const claim = railClaim(rail, chain.els, p)
      if (claim === null) continue
      for (const r of rules) {
        if (r.depth > 0 && r.at > open && r.at < end) continue
        if (below1280(r)) continue
        const s = chainSpec(r.sel, chain.els)
        if (s === null || prop(r.body, p) === '') continue
        if (specGt(s, claim.spec))
          out.push(
            `${chain.name} ${p}: ${where(r.sel)} (${s.join(',')}) outweighs the rail's ${where(claim.rule.sel)} (${claim.spec.join(',')})`,
          )
        else if (specEq(s, claim.spec) && r.at > claim.rule.at)
          out.push(
            `${chain.name} ${p}: ${where(r.sel)} (${s.join(',')}) ties the rail's ${where(claim.rule.sel)} (${claim.spec.join(',')}) and is written after it`,
          )
      }
    }
  return out
}

// Every short-viewport rule (the max-height block, the one narrower window
// that applies at 1280 px wide) that the rail would beat on the same element
// and property: the rail must keep the H5c.2 static bar on a short viewport.
const railWinsOnShort = (css: string): string[] => {
  const [open, end] = railRanges(css)
  const rules = rulesOf(css)
  const rail = rules.filter((r) => r.depth > 0 && r.at > open && r.at < end)
  const shorts = atRangesIn(css, SHORT)
  const out: string[] = []
  for (const chain of CHAINS)
    for (const p of RAIL_PROPS) {
      const claim = railClaim(rail, chain.els, p)
      if (claim === null) continue
      for (const r of rules) {
        if (!(
          r.depth > 0 &&
          shorts.some(([a, e]) => r.at > a && r.at < e && prop(r.body, p))
        ))
          continue
        const s = chainSpec(r.sel, chain.els)
        if (s === null) continue
        if (specGt(claim.spec, s))
          out.push(
            `${chain.name} ${p}: the rail's ${where(claim.rule.sel)} (${claim.spec.join(',')}) outweighs ${where(r.sel)} (${s.join(',')}) of the short-viewport block`,
          )
        else if (specEq(claim.spec, s) && claim.rule.at > r.at)
          out.push(
            `${chain.name} ${p}: the rail's ${where(claim.rule.sel)} (${claim.spec.join(',')}) ties ${where(r.sel)} (${s.join(',')}) of the short-viewport block and is written after it`,
          )
      }
    }
  return out
}

test('H9b cascade: no rule that applies at 1280 px beats the rail on its properties, and the rail beats no rule of the short-viewport block (the self-probes are the teeth of both scans)', () => {
  // The specificity helper first, on the spellings this slice's selectors
  // count by (:has takes its argument, CSS Selectors 4 7.2.10).
  expect(specOf(SEC_SEL), 'section:has(> .jump) is 0,1,1').toEqual([0, 1, 1])
  expect(specOf(RAIL_SEL), 'section:has(> .jump) > .jump is 0,2,1').toEqual([
    0, 2, 1,
  ])
  expect(specOf('main > *'), 'main > * is 0,0,1').toEqual([0, 0, 1])
  expect(
    specOf('.jump button'),
    '.jump button is the 0,1,1 of the brief',
  ).toEqual([0, 1, 1])
  // Scan 1: nothing beats the rail at 1280 px and above.
  expect(
    railLoses(
      `${RAIL1280} { ${SEC_SEL} { display: grid } } .layout ${SEC_SEL} { display: block }`,
    ),
    'a class added to the chain is a higher specificity: the grid never lands, the section stays a block, the rail has no column: the scan names it',
  ).toEqual([
    "the page section display: .layout section:has(> .jump) (0,2,1) outweighs the rail's section:has(> .jump) (0,1,1)",
  ])
  expect(
    railLoses(
      `${RAIL1280} { ${SEC_SEL} { display: grid } } ${SHORT} { ${SEC_SEL} { display: flex } }`,
    ),
    'the same specificity in a block that applies at 1280 px wide, written after: on a short viewport the sheet order decides, and the rail loses: the scan names it',
  ).toEqual([
    "the page section display: section:has(> .jump) (0,1,1) ties the rail's section:has(> .jump) (0,1,1) and is written after it",
  ])
  expect(
    railLoses(
      `${RAIL1280} { ${RAIL_SEL} { position: sticky } } .layout section .jump { position: relative }`,
    ),
    'a tie on the rail nav decided by the order of the sheet: the sticky rail is what H9b.2 is, so a later tie is the violation',
  ).toEqual([
    "the rail (nav.jump) position: .layout section .jump (0,2,1) ties the rail's section:has(> .jump) > .jump (0,2,1) and is written after it",
  ])
  expect(
    railLoses(
      `${RAIL1280} { ${SEC_SEL} { display: grid } } @media (max-width: 959.98px) { .layout ${SEC_SEL} { display: block } }`,
    ),
    'a rule of a window that ends below 1280 px can never meet the rail: not a violation, and the scan does not invent one',
  ).toEqual([])
  expect(
    railLoses(
      `${RAIL1280} { ${SEC_SEL} { display: grid } } .footer { display: block }`,
    ),
    'a rule reaching none of the rail elements is none of this pin',
  ).toEqual([])
  expect(
    railLoses(
      `${RAIL1280} { ${SEC_SEL} { max-width: calc(880px + 15rem) } } main > * { max-width: 880px }`,
    ),
    'the cap of main > * is (0,0,1) and loses: the scan of the real sheet must list it as nothing, and this is that shape read back',
  ).toEqual([])
  // Scan 2: the rail never beats the short-viewport block.
  expect(
    railWinsOnShort(
      `${RAIL1280} { section:has(> .jump) > .save-bar { position: sticky } } ${SHORT} { .save-bar { position: static } }`,
    ),
    'the rail putting a sticky bar on a short viewport beats H5c.2: the brief refuses it (not chosen, section 0), and the scan names it',
  ).toEqual([
    "the save bar position: the rail's section:has(> .jump) > .save-bar (0,2,1) outweighs .save-bar (0,1,0) of the short-viewport block",
  ])
  expect(
    railWinsOnShort(
      `${SHORT} { .save-bar { position: static } } ${RAIL1280} { section:has(> .jump) > .save-bar { position: sticky } }`,
    ),
    'the same beat written before the short block: specificity decides before order, so the order buys nothing',
  ).toEqual([
    "the save bar position: the rail's section:has(> .jump) > .save-bar (0,2,1) outweighs .save-bar (0,1,0) of the short-viewport block",
  ])
  expect(
    railWinsOnShort(
      `${RAIL1280} { .save-bar { position: sticky } } ${SHORT} { .save-bar { position: static } }`,
    ),
    'a tie written BEFORE the short block loses to it: the bar goes static on a short viewport, exactly as it must, and the scan stays silent',
  ).toEqual([])
  expect(
    railWinsOnShort(
      `${RAIL1280} { ${RAIL_SEL} { top: var(--space-4) } } ${SHORT} { .save-bar { bottom: 0 } }`,
    ),
    'a rail property on the nav and a short-viewport property on the bar are different elements: nothing to weigh',
  ).toEqual([])
  // The shape the slice is meant to end in, as a sheet of its own: the rail
  // rules of H9b.1 and H9b.2, the cap of main > *, the static bar of the short
  // block. Both scans must stay silent on it, which is what the two pins on
  // the sheet below ask for.
  const good =
    `${RAIL1280} { ${SEC_SEL} { display: grid; ` +
    'grid-template-columns: minmax(0, 880px) 15rem; column-gap: var(--space-6); ' +
    'align-items: start; max-width: calc(880px + 15rem + var(--space-6)) } ' +
    `${SEC_SEL} > * { grid-column: 1 } ${RAIL_SEL} { grid-column: 2; ` +
    'grid-row: 2 / span 40; position: sticky; top: var(--space-4); ' +
    'max-height: calc(100vh - var(--space-4)); overflow-y: auto; margin: 0 } } ' +
    `main > * { max-width: 880px } ${SHORT} { .save-bar { position: static } }`
  expect(
    railLoses(good),
    'the finished shape: the rail rules, the cap and the short block in one sheet, and the scan names nothing',
  ).toEqual([])
  expect(
    railWinsOnShort(good),
    'the finished shape, scan 2: the rail beats nothing of the short block',
  ).toEqual([])
  // The sheet itself.
  expect(
    railLoses(CSS),
    'H9b: no rule of the sheet that applies at 1280 px wide outweighs the rail on display, grid-template-columns, position, top or max-height on its section, its nav or the bar, neither by specificity nor by the order of the sheet at a tie',
  ).toEqual([])
  expect(
    railWinsOnShort(CSS),
    'H9b.5: the rail takes its own column and beats nothing of the short-viewport block, so the bar keeps its static form and its scroll padding there (2.4.11)',
  ).toEqual([])
})
