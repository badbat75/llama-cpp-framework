// fe/src/HearthH8a.test.ts
// H8a, the Lucide icons of the shell (fe/briefs/08-hearth-h8-icons.md), the
// stylesheet plane: H8a.4 the `.icon` box, H8a.5 the host rows that lay icon
// and text out (its cascade of the Menu row, and the nav entry that holds an
// icon, both from section 6 of the brief's review of H8a), H8a.6 forced
// colours, H8a.7 the target sizes. jsdom lays nothing
// out, so these are read as declarations the way HearthH2a.test.ts,
// HearthH5a.test.ts and HearthH5b.test.ts read them: comments dropped, the rules
// from the brace balance, the nesting from the balance before the selector. The
// reader here is a local copy of theirs, because a test file imports no other
// test file.
//
// H8a.1, H8a.2, H8a.3 and H8a.8 are DOM and live in HearthIcons.test.tsx. H8a.9
// (no key, the mark, the H4 and H5 pins) is carried by the mark count there and
// by HearthTheme.test.tsx, HearthBanner.test.tsx, HearthNav.test.tsx,
// HearthMark.test.tsx and App.test.tsx, none of which the slice may move.
//
// H8a.6 and H8a.7 are absences, so they are green the day they are written: they
// are the guards the slice must not break, and each carries its own self-probe
// over inline CSS, which is what proves the scan has teeth. The cascade pin of
// H8a.5 carries self-probes over short sheets for the same reason: the sheet it
// scans is the one the fix has to land in, so a scan that reports the doctored
// `#menu-button` sheet is what makes its green mean anything.
//
// OPEN, for the designer's UX pass: how the icon lines up with the text beside
// it, and what it looks like in forced colours.
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

// The rules of a CSS text: each with its selector in the three spellings the
// pins match, its body, its offset and its nesting level (0 at the top level,
// counted from the braces before it). The cascade pin runs its scan on
// index.css AND on the short sheets its self-probes write, so the reader takes
// the text: comments are dropped as spaces, which keeps the offsets of the
// text it was given. RULES is the sheet's.
const rulesOf = (css: string): Rule[] => {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, (m) => ' '.repeat(m.length))
  const opened = (upto: number): number =>
    (stripped.slice(0, upto).match(/\{/g) ?? []).length -
    (stripped.slice(0, upto).match(/\}/g) ?? []).length
  return [...stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    sel: m[1].trim(),
    // Whitespace dropped: the selector is written over several lines.
    compact: m[1].replace(/\s+/g, ''),
    // Runs of whitespace become one space, so a needle can name a descendant
    // combinator (".session button"), which compact cannot.
    flat: m[1].trim().replace(/\s+/g, ' '),
    body: m[2],
    at: m.index ?? 0,
    depth: opened(m.index ?? 0),
  }))
}

const RULES: Rule[] = rulesOf(CSS)

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

// The names a block declares, for a "the brief lists exactly these properties"
// check: an extra declaration is a red, not a free choice.
const declNames = (body: string): string[] =>
  body
    .split(';')
    .map((d) => {
      const i = d.indexOf(':')
      return i < 0 ? '' : d.slice(0, i).trim()
    })
    .filter((n) => n !== '')
    .sort()

// Like HearthH2a's `one`, usable from module scope and from helpers, where
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

// index.css is written with no space between a compound's parts, so a
// descendant selector is given to the matchers with its space dropped.
const tight = (sel: string): string => sel.replace(/ /g, '')

// The end of the at-rule block that starts at this offset, by its matched
// closing brace.
const rangeEndIn = (css: string, open: number): number => {
  let end = open
  let depth = 0
  for (; end < css.length; end++) {
    if (css[end] === '{') depth++
    else if (css[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

// Every block of this CSS text written with this at-rule header, as
// [open, end] offsets.
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

// The rules inside every block with this header whose selector holds `has`.
const insideIn = (css: string, needle: string, has: string): Rule[] => {
  const hits: Rule[] = []
  for (const [open, end] of atRangesIn(css, needle))
    for (const r of rulesOf(css))
      if (r.depth > 0 && r.at > open && r.at < end && r.compact.includes(has))
        hits.push(r)
  return hits
}

// The scan bound to index.css.
const inside = (needle: string, has: string): Rule[] =>
  insideIn(CSS, needle, has)

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

// A selector's states and elements, counted the way HearthH2a counts
// specificity: (a) the ids, attribute selectors, classes and pseudo-classes,
// (b) the elements and pseudo-elements. A combinator is neither, so it goes in
// the signature and never in (b).
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

// `a` wins over `b` in the cascade: the (a) part first, then (b).
const outweighs = (a: Sel, b: Sel): boolean =>
  a.spec[0] > b.spec[0] || (a.spec[0] === b.spec[0] && a.spec[1] >= b.spec[1])

// A selector part that carries `token` as a class, an id, an attribute or a
// pseudo-class: matched WHOLE, so `.iconology` and `.icon-toggle` are not
// `.icon`, and `.markdown` is not `.mark`.
const hasToken = (raw: string, token: string): boolean =>
  parseSel(raw).states.includes(token)

// The light token block's custom properties.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

const lightTokens = (): [string, string][] =>
  customProps(topBy(':root', 'the light token block').body)

// The value of one token; a miss throws naming the token, so a pin that lacks a
// token goes red on the token, not on NaN.
const valueOf = (pairs: [string, string][], name: string): string => {
  const hits = pairs.filter((p) => p[0] === name).map((p) => p[1])
  if (hits.length !== 1)
    throw new Error(`${name} is declared ${hits.length} times in the block`)
  return hits[0]
}

// The blocks a CSS text holds, at any nesting (HearthH2a's helper): the rules of
// the top level and those inside an at-rule. The at-rule itself is one of them,
// flagged by the `@` its selector starts with, and it holds no box of its own.
const blocksIn = (css: string): { selector: string; body: string }[] => {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, (m) => ' '.repeat(m.length))
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

// The name a block shows in a message: its selector, whitespace collapsed.
const where = (selector: string): string => selector.replace(/\s+/g, ' ').trim()

// ── H8a.4: the `.icon` box ────────────────────────────────────────────────

test('H8a.4: the top-level .icon rule is the 1.25em square of flex: none and declares nothing else, and .theme-switch svg keeps its 1.25rem box, which outweighs it', () => {
  const icon = topBy('.icon', 'the .icon rule (H8a.4)')
  expect(
    declNames(icon.body),
    'the three properties the brief lists, and no more: a width and a height in em, and the flex that keeps the icon from taking a share of the line',
  ).toEqual(['flex', 'height', 'width'])
  expect(
    prop(icon.body, 'width'),
    '1.25em, so the icon scales with the host text and with the user text size (1.4.4)',
  ).toBe('1.25em')
  expect(prop(icon.body, 'height'), 'the square (section 0)').toBe('1.25em')
  expect(
    prop(icon.body, 'flex'),
    'none: pressing a button that holds an icon moves no text',
  ).toBe('none')
  const sw = topBy(
    tight('.theme-switch svg'),
    'the theme switch icon box (H4.11)',
  )
  expect(
    prop(sw.body, 'width'),
    'the H4 rule stays: 1.25rem on the theme button, as the brief keeps it',
  ).toBe('1.25rem')
  expect(prop(sw.body, 'height'), 'and its height').toBe('1.25rem')
  expect(
    outweighs(parseSel('.theme-switch svg'), parseSel('.icon')),
    '0,1,1 over 0,1,0: the switch box wins on the theme button by its own specificity, so the answer needs no source order and no media query',
  ).toBe(true)
})

// ── H8a.5: the hosts lay icon and text out ────────────────────────────────

const NARROW = '@media (max-width: 479.98px)'

test('H8a.5: the Menu button and Sign out become a centred inline-flex row at the space-2 gap, the Sign in link and the nav pills add the same gap to the row they already are, and the narrow block touches no Menu rule while the wide one keeps hiding it', () => {
  const row = flatBy(
    '.menu-button, .session button',
    'the icon-and-text row of the Menu button and of Sign out (H8a.5)',
  )
  expect(
    listItems(row.sel),
    'the selector list the brief spells: those two selectors, in that order, nothing else (the correction after the H8a review: the id is forbidden here, it outweighs the wide hide)',
  ).toEqual(['.menu-button', '.session button'])
  expect(prop(row.body, 'display'), 'a row').toBe('inline-flex')
  expect(
    prop(row.body, 'align-items'),
    'the icon centred on the text it sits beside',
  ).toBe('center')
  expect(prop(row.body, 'gap'), 'the small gap of the tokens').toBe(
    'var(--space-2)',
  )
  for (const [sel, display] of [
    ['a.session', 'inline-flex'],
    ['.nav a', 'flex'],
  ] as [string, string][]) {
    const host = topBy(tight(sel), `the ${sel} rule (H5a.4, H5b.2)`)
    expect(
      prop(host.body, 'gap'),
      `${sel}: the same gap added to the row it already is`,
    ).toBe('var(--space-2)')
    expect(
      prop(host.body, 'display'),
      `${sel}: the display H5 gave it is kept, not replaced`,
    ).toBe(display)
    expect(
      prop(host.body, 'min-height'),
      `${sel}: the 32 px pill of H5a.4 and H5b.2 is kept`,
    ).toBe('32px')
  }
  expect(
    valueOf(lightTokens(), '--space-2'),
    'the gap is the token, not a literal',
  ).toBe('8px')
  for (const needle of ['#menu-button', '.menu-button'])
    expect(
      inside(NARROW, needle),
      `no rule inside the ${NARROW} block reaches ${needle}: the Menu keeps its text at every width, as H5 decided and H8a.5 keeps`,
    ).toEqual([])
  const wide = inside('@media (min-width: 960px)', '.menu-button')
  expect(
    wide.length,
    'the wide block still has its one rule reaching the Menu button',
  ).toBe(1)
  expect(
    prop(wide[0]!.body, 'display'),
    'and it is still the display: none that hides it on a wide screen (H5b.1)',
  ).toBe('none')
})

// ── H8a.5, the cascade of the Menu row (defect 1 of the review) ──────────

const WIDE = '@media (min-width: 960px)'

// A selector's specificity as the cascade counts it, in three parts: the ids
// (1,0,0), then the classes, the attribute selectors and the pseudo-classes
// (0,1,0), then the elements and the pseudo-elements (0,0,1). parseSel above
// adds the ids and the classes into one number, and that is exactly what hid
// the defect this pin has to catch, so these pins count the three parts. A
// functional pseudo-class counts as one (0,1,0) with its argument folded in:
// the sheet uses `:has(> .icon)`, whose argument is a single class, so the
// count comes out the same either way.
type Spec = [number, number, number]

const specOf = (raw: string): Spec => {
  const s: Spec = [0, 0, 0]
  for (const m of raw.trim().matchAll(TOKEN)) {
    const t = m[0]
    if (/^\s/.test(t) || t === '*' || t === '+' || t === '>' || t === '~')
      continue
    if (t[0] === '#') s[0]++
    else if (t.startsWith('::')) s[2]++
    else if (t[0] === '.' || t[0] === '[' || t[0] === ':') s[1]++
    else s[2]++
  }
  return s
}

// `a` over `b`, most significant part first.
const specGt = (a: Spec, b: Spec): boolean =>
  a[0] !== b[0] ? a[0] > b[0] : a[1] !== b[1] ? a[1] > b[1] : a[2] > b[2]

// The same specificity: only a tie is decided by the order of the sheet, a rule
// that LOSES on specificity stays lost whatever the order.
const specEq = (a: Spec, b: Spec): boolean =>
  a[0] === b[0] && a[1] === b[1] && a[2] === b[2]

// The Menu button as App.tsx renders it (H8a.3), from the element outwards: the
// FIRST child of div.banner-tools, under header.banner, under the mount point.
type El = { tag: string; id: string; classes: string[] }

const MENU: El[] = [
  { tag: 'button', id: 'menu-button', classes: ['menu-button', 'secondary'] },
  { tag: 'div', id: '', classes: ['banner-tools'] },
  { tag: 'header', id: '', classes: ['banner'] },
  { tag: 'div', id: 'root', classes: [] },
  { tag: 'body', id: '', classes: [] },
  { tag: 'html', id: '', classes: [] },
]

// A selector part split into its compounds at the whitespace and the
// combinators OUTSIDE brackets and parentheses: the space in `:has(> .icon)`
// is not a combinator. Each compound carries the combinator that precedes it.
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
// rule carrying one is scanned rather than skipped, the safe direction for a
// rule that must not win.
const compoundOf = (text: string): El & { opaque: number } => {
  const out = { tag: '', id: '', classes: [] as string[], opaque: 0 }
  for (const m of text.matchAll(TOKEN)) {
    const t = m[0]
    if (t[0] === '#') out.id = t.slice(1)
    else if (t[0] === '.') out.classes.push(t.slice(1))
    else if (t[0] === '[' || t[0] === ':') out.opaque++
    else if (t !== '*') out.tag = t
  }
  return out
}

const fits = (c: El, el: El): boolean =>
  (c.tag === '' || c.tag === el.tag) &&
  (c.id === '' || c.id === el.id) &&
  c.classes.every((k) => el.classes.includes(k))

// Does this selector part reach the Menu button? Its LAST compound must fit the
// button, then the compounds before it are walked up MENU: a descendant may be
// any ancestor, a child must be the one, and a sibling combinator can never
// reach the button, which is the first child of its parent.
const matchesMenu = (raw: string): boolean => {
  const chain = chainOf(raw)
  if (chain.length === 0) return false
  if (!fits(compoundOf(chain[chain.length - 1].text), MENU[0])) return false
  let i = 1
  for (let k = chain.length - 2; k >= 0; k--) {
    const c = compoundOf(chain[k].text)
    const comb = chain[k + 1].comb
    if (comb === '+' || comb === '~') return false
    if (comb === '>') {
      if (i >= MENU.length || !fits(c, MENU[i])) return false
      i++
      continue
    }
    while (i < MENU.length && !fits(c, MENU[i])) i++
    if (i >= MENU.length) return false
    i++
  }
  return true
}

// The highest specificity among the parts of a selector list that reach the
// Menu button; null when none does. The specificity of a list is that of the
// part that matches, not of the loudest part it also carries.
const menuSpec = (sel: string): Spec | null => {
  let best: Spec | null = null
  for (const part of listItems(sel)) {
    if (!matchesMenu(part)) continue
    const s = specOf(part)
    if (best === null || specGt(s, best)) best = s
  }
  return best
}

// Every top-level rule of a CSS text that sets `display` on the Menu button and
// wins against the wide hide `.menu-button { display: none }` of H5b.1: a
// higher specificity, or the same specificity written AFTER the hide, since a
// media query adds no specificity and the sheet's order decides. An empty list
// is H8a.5 met: the Menu stays hidden at 960 px and above. A `!important` is
// not read, the sheet uses none on the Menu.
const menuCascade = (css: string): string[] => {
  const ranges = atRangesIn(css, WIDE)
  if (ranges.length === 0)
    throw new Error(
      `expected the ${WIDE} block that hides the Menu button (H5b.1): the icon row is written to lose to it, found none`,
    )
  const [open, end] = ranges[0]
  const rules = rulesOf(css)
  const hides = rules.filter(
    (r) =>
      r.depth > 0 &&
      r.at > open &&
      r.at < end &&
      prop(r.body, 'display') === 'none' &&
      menuSpec(r.sel) !== null,
  )
  if (hides.length !== 1)
    throw new Error(
      `expected exactly one rule of ${WIDE} hiding the Menu button, found ${hides.length}`,
    )
  const hide = hides[0]
  // The filter above kept only the rules a part of which reaches the button, so
  // the specificity of that part is there to read.
  const hideSpec = menuSpec(hide.sel)!
  const out: string[] = []
  for (const r of rules) {
    if (r.depth !== 0) continue
    const v = prop(r.body, 'display')
    if (v === '') continue
    const s = menuSpec(r.sel)
    if (s === null) continue
    if (specGt(s, hideSpec))
      out.push(
        `${where(r.sel)}: display: ${v} at ${s.join(',')} outweighs the wide hide at ${hideSpec.join(',')}`,
      )
    else if (specEq(s, hideSpec) && r.at > hide.at)
      out.push(
        `${where(r.sel)}: display: ${v} at ${s.join(',')} ties the wide hide at ${hideSpec.join(',')} and is written after it`,
      )
  }
  return out
}

test('H8a.5: no top-level rule that sets display on the Menu button wins against the wide hide, neither by specificity nor by the order of the sheet at a tie, so the Menu stays hidden at 960 px and above (the self-probes are the teeth of the scan)', () => {
  // The scan, on sheets written here.
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } #menu-button { display: inline-flex }`,
    ),
    'the defect the review measured in three engines: the id is (1,0,0) and the hide (0,1,0), so the row was in force at every width and the Menu showed on a wide screen',
  ).toEqual([
    '#menu-button: display: inline-flex at 1,0,0 outweighs the wide hide at 0,1,0',
  ])
  expect(
    menuCascade(
      `#menu-button { display: inline-flex } ${WIDE} { .menu-button { display: none } }`,
    ),
    'the same id written BEFORE the hide is just as broken: specificity decides before order',
  ).toEqual([
    '#menu-button: display: inline-flex at 1,0,0 outweighs the wide hide at 0,1,0',
  ])
  expect(
    menuCascade(
      `.menu-button { display: inline-flex } ${WIDE} { .menu-button { display: none } }`,
    ),
    'the shape the brief now gives: (0,1,0) ties (0,1,0) and is written before, so the hide wins and the Menu is hidden from 960 px',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .menu-button { display: inline-flex }`,
    ),
    'the same tie written after the hide wins it, so the row must stay above the block',
  ).toEqual([
    '.menu-button: display: inline-flex at 0,1,0 ties the wide hide at 0,1,0 and is written after it',
  ])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .banner-tools .menu-button { display: inline-flex }`,
    ),
    'a class added to the compound is a higher specificity too, so the fix is that one class and not a chain',
  ).toEqual([
    '.banner-tools .menu-button: display: inline-flex at 0,2,0 outweighs the wide hide at 0,1,0',
  ])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .banner .menu-button { display: inline-flex }`,
    ),
    'an ancestor reached by a descendant combinator is a rule that reaches the button',
  ).toEqual([
    '.banner .menu-button: display: inline-flex at 0,2,0 outweighs the wide hide at 0,1,0',
  ])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .banner > .menu-button { display: inline-flex }`,
    ),
    'the banner is the parent of the tools, not of the button: a child combinator there reaches nothing, so the scan lists no rule',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .layout .menu-button { display: inline-flex }`,
    ),
    'the button is not under .layout: the ancestor walk is real, and a rule about a box the button is not in is not a violation',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .theme-switch ~ .menu-button { display: inline-flex }`,
    ),
    'the Menu is the first child of the tools, so a sibling combinator can never reach it',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } button { display: inline-flex }`,
    ),
    'the shared button rule is (0,0,1): it loses to the hide, which is why the row of H8a.5 is free to carry the display of a control',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .menu-button { color: var(--accent) }`,
    ),
    'a rule reaching the button that sets no display is none of this pin',
  ).toEqual([])
  expect(
    menuCascade(
      `${WIDE} { .menu-button { display: none } } .markdown { display: flex }`,
    ),
    'a rule reaching nothing of the shell is not listed',
  ).toEqual([])
  expect(
    () => menuCascade('.menu-button { display: inline-flex }'),
    'a sheet with no wide hide is a broken fixture, not a pass: the throw names the block',
  ).toThrow(`expected the ${WIDE} block`)
  expect(
    menuCascade(CSS),
    'H8a.5: nothing in the sheet beats the wide hide on the Menu button, so the Menu is hidden at 960 px and above. The review measured a 104 by 40 Menu at 1280 px in Chromium, Firefox and WebKit, because the row of H8a.5 was written `#menu-button`',
  ).toEqual([])
})

// ── H8a.5, the nav entry that holds an icon (defect 2 of the review) ─────

test('H8a.5: a nav entry that holds an icon wraps its label whole under the icon and breaks inside a word only when a word alone has no room, while a plain nav entry keeps the break a long instance id needs', () => {
  const entry = topBy(
    tight('.nav a:has(> .icon)'),
    'the nav entry that holds an icon (H8a.5)',
  )
  expect(
    declNames(entry.body),
    'the two properties the brief lists, and no more',
  ).toEqual(['flex-wrap', 'overflow-wrap'])
  expect(
    prop(entry.body, 'flex-wrap'),
    'the label moves under the icon when it does not fit beside it: the review measured "Overvi / ew" and "Mode / l profil / es" at 200 % text in the 224 px column',
  ).toBe('wrap')
  expect(
    prop(entry.body, 'overflow-wrap'),
    'break-word: a break inside a word only where a whole word has no room, not at every character',
  ).toBe('break-word')
  expect(
    specGt(specOf('.nav a:has(> .icon)'), specOf('.nav a')),
    '0,2,1 over 0,1,1: the icon rule wins on the entries it names, so the plain rule stays in force on the instance links, which is what the brief splits',
  ).toBe(true)
  expect(
    prop(
      topBy(tight('.nav a'), 'the .nav a rule (H5b.2)').body,
      'overflow-wrap',
    ),
    'anywhere stays on the plain entry: an instance id of 32 characters has no break point, and a break-word there makes the whole id one unbreakable word (CSS Text 3, overflow-wrap)',
  ).toBe('anywhere')
})

// ── H8a.6: forced colours, the icon follows its host ──────────────────────

// The properties that would give an icon a colour of its own.
const TINT = ['color', 'fill', 'stroke', 'background', 'background-color']

// The blocks at ONE level of a CSS text: the loop `blocksIn` runs at each
// level, exposed, so a scan can see the context (inside a forced-colours block
// or not) the sanctioned pair of H8b.4 is keyed on.
const levelIn = (text: string): { selector: string; body: string }[] => {
  const out: { selector: string; body: string }[] = []
  let i = 0
  for (;;) {
    const open = text.indexOf('{', i)
    if (open < 0) return out
    let end = open + 1
    let depth = 1
    while (end < text.length && depth > 0) {
      if (text[end] === '{') depth++
      else if (text[end] === '}') depth--
      if (depth > 0) end++
    }
    if (depth > 0) return out
    out.push({
      selector: text.slice(i, open).trim(),
      body: text.slice(open + 1, end),
    })
    i = end + 1
  }
}

// H8b.4 sanctions exactly two colour declarations on the filter icon: the
// `--muted` tint at the top level and the `color: inherit` of its
// forced-colours block, each carried alone. The exemption is BY THE
// DECLARATIONS: the same selectors with any other tint, or with a second one,
// are reported, so a rule that only looks sanctioned is no exemption.
const sanctionedTint = (tints: [string, string][], forced: boolean): boolean =>
  tints.length === 1 &&
  tints[0][0] === 'color' &&
  tints[0][1] === (forced ? 'inherit' : 'var(--muted)')

// Every block that reaches `.icon` and declares one of them, in any nesting,
// including inside an at-rule (a forced-colours block is one), the sanctioned
// pair of H8b.4 excepted by the declarations it carries.
const tintedIn = (css: string): string[] => {
  const out: string[] = []
  const walk = (text: string, forced: boolean): void => {
    for (const b of levelIn(text)) {
      if (b.selector.startsWith('@')) {
        walk(b.body, forced || /forced-colors/.test(b.selector))
        continue
      }
      if (!listItems(b.selector).some((raw) => hasToken(raw, '.icon'))) continue
      const tints = TINT.map(
        (n) => [n, prop(b.body, n)] as [string, string],
      ).filter(([, v]) => v !== '')
      if (tints.length === 0) continue
      if (sanctionedTint(tints, forced)) continue
      for (const [name, v] of tints)
        out.push(`${where(b.selector)}: ${name}: ${v}`)
    }
  }
  walk(css, false)
  return out
}

test('H8a.6: no .icon rule sets a colour, in or out of a forced-colours block, the filter icon of H8b.4 carrying only its sanctioned tint and its sanctioned inherit (the self-probes are the teeth of the scan)', () => {
  // The scan first, on CSS written here: what it must catch, and what a name
  // that only looks like `.icon` must not catch.
  expect(
    tintedIn('.icon { color: red }'),
    'a colour of its own on the icon is the violation',
  ).toEqual(['.icon: color: red'])
  expect(
    tintedIn('.notice > .icon { fill: CanvasText }'),
    'a descendant of another selector is still a rule reaching the icon',
  ).toEqual(['.notice > .icon: fill: CanvasText'])
  expect(
    tintedIn('@media (forced-colors: active) { .icon { color: CanvasText } }'),
    'the inside of a forced-colours block is read, not skipped',
  ).toEqual(['.icon: color: CanvasText'])
  expect(
    tintedIn('.icon { background: var(--accent-soft) }'),
    'a fill counts as a colour of its own',
  ).toEqual(['.icon: background: var(--accent-soft)'])
  for (const css of [
    '.iconology { color: red }',
    '.icon-toggle { stroke: red }',
    '.markdown { color: red }',
  ])
    expect(
      tintedIn(css),
      `"${where(css)}" is no rule reaching .icon: a token matched whole, so a class that only starts like it is not exempt and not a hit`,
    ).toEqual([])
  expect(
    tintedIn(
      '.model-profiles-filter-row > .icon { position: absolute; color: var(--muted) }',
    ),
    'H8b.4: the top-level tint of the filter icon, read back exempt, with its placement declarations around it',
  ).toEqual([])
  expect(
    tintedIn(
      '@media (forced-colors: active) { .model-profiles-filter-row > .icon { color: inherit } }',
    ),
    'H8b.4: the forced-colours inherit, read back exempt inside the block',
  ).toEqual([])
  expect(
    tintedIn(
      '@media (forced-colors: active) { .model-profiles-filter-row > .icon { color: CanvasText } }',
    ),
    'the exemption is by the declaration, not the selector name: the filter icon selectors with another colour are a hit',
  ).toEqual(['.model-profiles-filter-row > .icon: color: CanvasText'])
  expect(
    tintedIn('.model-profiles-filter-row > .icon { color: inherit }'),
    'the forced value outside the block is a tint of its own there: inherit is sanctioned only inside',
  ).toEqual(['.model-profiles-filter-row > .icon: color: inherit'])
  expect(
    tintedIn(
      '.model-profiles-filter-row > .icon { color: var(--muted); fill: currentColor }',
    ),
    'a sanctioned tint is carried alone: a second tint on the same rule is a hit',
  ).toEqual([
    '.model-profiles-filter-row > .icon: color: var(--muted)',
    '.model-profiles-filter-row > .icon: fill: currentColor',
  ])
  expect(
    tintedIn(CSS),
    'H8a.6, kept by H8b.4: the icon has no colour of its own anywhere in the sheet, except the filter icon of H8b.4 carrying its --muted tint at the top level and its inherit inside the forced-colours block (section 0)',
  ).toEqual([])
})

// ── H8a.7: the targets keep their size ────────────────────────────────────

// A selector part that reaches the Sign out button: a button under `.session`
// (the brief's spelling is `.session button`; a child combinator or a state on
// the button reaches the same element).
const isSignOut = (raw: string): boolean => {
  const s = parseSel(raw)
  return (
    s.states.includes('.session') &&
    s.sig.replace(/\s+/g, '').endsWith('button')
  )
}

// A selector part that reaches one of the two banner controls H8a.7 names.
const isTarget = (raw: string): boolean =>
  hasToken(raw, '#menu-button') ||
  hasToken(raw, '.menu-button') ||
  isSignOut(raw)

// A size in px: `Npx`, or a custom property resolved through the block's own
// tokens; null for anything the text does not state (a percentage, a rem of the
// root, calc), which counts as a miss, not as a pass.
const sizeOf = (value: string, tokens: Map<string, string>): number | null => {
  const px = /^(\d+(?:\.\d+)?)px$/.exec(value)
  if (px) return Number.parseFloat(px[1])
  const token = /^var\((--[a-z0-9-]+)\)$/.exec(value)
  if (token) {
    const hit = tokens.get(token[1])
    return hit === undefined ? null : sizeOf(hit, tokens)
  }
  return null
}

// Every height a rule reaching the Menu button or Sign out declares, under 40 px
// or unreadable. The heights are read in every nesting, so a media rule cannot
// shrink a target out of sight.
const undersized = (css: string): string[] => {
  const tokens = new Map<string, string>()
  for (const b of blocksIn(css))
    if (where(b.selector) === ':root')
      for (const [k, v] of customProps(b.body)) tokens.set(k, v)
  const out: string[] = []
  for (const b of blocksIn(css)) {
    if (b.selector.startsWith('@')) continue
    if (!listItems(b.selector).some(isTarget)) continue
    for (const name of ['height', 'min-height', 'max-height']) {
      const v = prop(b.body, name)
      if (!v) continue
      const px = sizeOf(v, tokens)
      if (px === null)
        out.push(`${where(b.selector)}: ${name}: ${v} (no size in px)`)
      else if (px < 40)
        out.push(`${where(b.selector)}: ${name}: ${v} (${px} px)`)
    }
  }
  return out
}

test('H8a.7: the Menu button and Sign out keep the 40 px control height, the Sign in link and the nav pills at least 32 px, and no rule reaching the Menu button or Sign out sizes them under 40 px, the media rules included (the self-probes are the teeth of the scan)', () => {
  expect(valueOf(lightTokens(), '--control-h'), 'the 40 px control token').toBe(
    '40px',
  )
  expect(
    prop(topBy('button', 'the button rule (H2a.1)').body, 'min-height'),
    'the Menu button and Sign out take the shared control height',
  ).toBe('var(--control-h)')
  for (const sel of ['a.session', '.nav a'])
    expect(
      prop(topBy(tight(sel), `the ${sel} rule`).body, 'min-height'),
      `${sel}: 32 px, the pill of H5a.4 and H5b.2, kept for 2.5.8`,
    ).toBe('32px')
  // The scan, on CSS written here.
  expect(
    undersized('#menu-button { min-height: 24px }'),
    'a 24 px Menu button is the violation this catches',
  ).toEqual(['#menu-button: min-height: 24px (24 px)'])
  expect(
    undersized('.session button { max-height: 30px }'),
    'Sign out is a target too, and a max-height caps the target just as well',
  ).toEqual(['.session button: max-height: 30px (30 px)'])
  expect(
    undersized('.session > button { height: 28px }'),
    'the same element reached by a child combinator is a target',
  ).toEqual(['.session > button: height: 28px (28 px)'])
  expect(
    undersized(
      '@media (max-width: 479.98px) { #menu-button { height: 28px } }',
    ),
    'a size inside a media rule is read, not skipped',
  ).toEqual(['#menu-button: height: 28px (28 px)'])
  expect(
    undersized(
      ':root { --control-h: 40px } #menu-button { min-height: var(--control-h) }',
    ),
    'a size the tokens state is read through them',
  ).toEqual([])
  expect(
    undersized(':root { --control-h: 40px } .markdown { height: 10px }'),
    'a rule that reaches neither target is not listed',
  ).toEqual([])
  expect(
    undersized('#menu-button { min-height: 100% }'),
    'a size the text does not state is a miss, not a pass',
  ).toEqual(['#menu-button: min-height: 100% (no size in px)'])
  expect(
    undersized(CSS),
    'H8a.7: nothing in the sheet takes the Menu button or Sign out under the 40 px control, the media rules included (2.5.8)',
  ).toEqual([])
})

// ── H8a.10: the current entry keeps its label beside its icon ────────────

// A value is compared with its whitespace collapsed: a long value is written
// over several lines by the formatter, and a wrap is not the code.
const oneLine = (v: string): string => v.replace(/\s+/g, ' ').trim()

test('H8a.10: inside the wide block the .nav rule gives the column no right padding so the current pill has room for its label beside the icon, while the pill padding, the gap, the 32 px height, the wrap of the icon entry and the 224 px column stay as they are', () => {
  const top = topBy('.nav', 'the top-level .nav rule (H5b.1)')
  const wide = single(
    inside(WIDE, '.nav').filter((r) => r.compact === tight('.nav')),
    'the .nav rule inside the @media (min-width: 960px) block (H8a.10)',
  )
  expect(
    declNames(wide.body),
    'the two properties of the follow-up, and no more: the divider stays gone and the column loses its right padding',
  ).toEqual(['border-bottom', 'padding-right'])
  expect(
    prop(wide.body, 'border-bottom'),
    'H5b.1: the wide shell keeps no divider',
  ).toBe('0')
  expect(
    oneLine(prop(wide.body, 'padding-right')),
    'H8a.10: the cause measured at 1280 px is the column padding, 224 less 2 x 16, which leaves 138 inside the pill for a 600-weight label of about 116, so flex-wrap moved the label under the icon. Nothing draws a line at the column edge and main keeps its own inline padding, so the pills reach the edge and the label fits beside the icon',
  ).toBe('0')
  expect(
    wide.at > top.at,
    'the wide rule has the specificity of the top-level one (0,1,0), so it wins over the padding shorthand by the order of the sheet: it must be written after it',
  ).toBe(true)
  // The guards the follow-up must not touch.
  expect(
    declNames(top.body),
    'the top-level rule keeps exactly its three properties: the follow-up belongs to the wide block, so the open menu below 960 px keeps both its paddings',
  ).toEqual(['background', 'border-bottom', 'padding'])
  expect(
    oneLine(prop(top.body, 'padding')),
    'H5a: the narrow open menu keeps its padding of space-3, space-4 and space-5',
  ).toBe('var(--space-3) var(--space-4) var(--space-5)')
  const pill = topBy(tight('.nav a'), 'the .nav a pill rule (H5b.2)')
  expect(
    oneLine(prop(pill.body, 'padding')),
    'the follow-up takes the column padding, never the pill padding: a narrower pill padding leaves under 1 px of slack across engines (the brief rejected it)',
  ).toBe('var(--space-1) var(--space-4)')
  expect(
    prop(pill.body, 'gap'),
    'the gap between the icon and the label stays the small gap of H8a.5 (the brief rejected shrinking it alone)',
  ).toBe('var(--space-2)')
  expect(
    prop(pill.body, 'min-height'),
    'and the pill stays a 32 px target (2.5.8)',
  ).toBe('32px')
  const entry = topBy(
    tight('.nav a:has(> .icon)'),
    'the nav entry that holds an icon (H8a.5)',
  )
  expect(
    prop(entry.body, 'flex-wrap'),
    'the wrap stays: at 200 % text the label moves whole under the icon, which is the fallback the follow-up keeps',
  ).toBe('wrap')
  expect(
    prop(entry.body, 'overflow-wrap'),
    'and it breaks inside a word only where a whole word has no room (section 6 of the brief)',
  ).toBe('break-word')
  const layout = single(
    inside(WIDE, '.layout').filter((r) => r.compact === tight('.layout')),
    'the .layout rule inside the @media (min-width: 960px) block (H5b.1)',
  )
  expect(
    oneLine(prop(layout.body, 'grid-template-columns')),
    'H8a.10: the nav column stays 224 px (a wider one narrows main on every page, rejected), so every page keeps its main width',
  ).toBe('224px minmax(0, 1fr)')
})
