// Hearth H10, the "On this page" label and the collapsed jump list
// (fe/briefs/08-hearth-h10-h11.md, section 1): the stylesheet plane. H10.6
// (narrow look) and H10.7 (wide look) are lists of declarations, so they are
// pinned as text of index.css the way HearthH9a.test.ts, HearthH9b.test.ts and
// HearthH8a.test.ts read it (comments dropped, rules from the brace balance,
// nesting from the balance before the selector); a test file imports no other
// test file, so the reader is a local copy.
//
// A declaration written in a rule the disclosure cannot escape is only half the
// brief: the toggle carries the .secondary class, and the top-level
// `.jump button` of H9a (0,1,1) outweighs `.jump-toggle` and `.secondary`
// (0,1,0) on display, min-height, border and border-radius. So the same
// declarations are weighed a second time as a cascade: for each element of the
// narrow page (the label, the toggle, the closed list) and each property the
// brief names, the rule with the highest claim (specificity, and at a tie the
// one written last) among the rules that apply at a narrow viewport: the top
// level and the max-width and max-height blocks, not the min-width, dark-scheme
// or forced-colours blocks. Today that scan is red on the toggle, and the red
// names `.jump button`: the coder has to scope the index entry rule so the
// disclosure keeps the pill it asks for.
//
// What the text cannot pin goes to the designer's UX pass (section 5): the
// chevron's turn, the ring whole around the toggle, the open list with no
// sideways scroll at 320 px, the label visible and no toggle at 1280 px. The
// DOM half of the slice (H10.1 to H10.5, H10.8, H10.10) is in
// HearthJumpToggle.test.tsx.
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
// offsets of the text the scan was given.
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

// The two media windows of the slice, spelled as the brief gives them, and the
// selectors of the three elements it styles.
const WIDE = '@media (min-width: 1280px)'
const NARROW = '@media (max-width: 1279.98px)'
const LABEL_SEL = '.jump-label'
const TOGGLE_SEL = '.jump-toggle'
const CHEVRON_SEL = ".jump-toggle[aria-expanded='true'] .icon"
const COLLAPSED_SEL = '.jump ul.collapsed'

// The rules of one at-rule block of a CSS text, by matched braces. The slice
// puts its wide rules in the one block H9b already has (H9b's own tests refuse a
// second 1280 px block), so a miss says so.
const blockRules = (css: string, header: string, what: string): Rule[] => {
  const ranges = atRangesIn(css, header)
  if (ranges.length !== 1)
    throw new Error(
      `expected exactly one ${header} block (${what}), found ${ranges.length}`,
    )
  const [open, end] = ranges[0]
  return rulesOf(css).filter((r) => r.depth > 0 && r.at > open && r.at < end)
}

// The rule of a block, or of the top level, whose selector list is exactly this
// selector.
const bySel = (rules: Rule[], sel: string, what: string): Rule =>
  single(
    rules.filter(
      (r) => listItems(r.sel).length === 1 && listItems(r.sel)[0] === sel,
    ),
    `the rule "${sel}" (${what})`,
  )

const topLevel = (css: string): Rule[] =>
  rulesOf(css).filter((r) => r.depth === 0)

// ── The cascade of the narrow page ───────────────────────────────────────

// A specificity in three parts: ids, then classes/attributes/pseudo-classes,
// then elements/pseudo-elements.
type Spec = [number, number, number]

const TOKEN =
  /\[[^\]]*\]|::[a-zA-Z-]+|:[a-zA-Z-]+(?:\([^)]*\))?|\.[a-zA-Z-]+|#[a-zA-Z-]+|[a-zA-Z][a-zA-Z0-9-]*|[*+>~]|\s+/g

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

const specGt = (a: Spec, b: Spec): boolean =>
  a[0] !== b[0] ? a[0] > b[0] : a[1] !== b[1] ? a[1] > b[1] : a[2] > b[2]

const specEq = (a: Spec, b: Spec): boolean =>
  a[0] === b[0] && a[1] === b[1] && a[2] === b[2]

// The elements the slice styles, as ModelProfilePage.tsx renders them, from the
// element outwards (the chain of HearthH9b's rail scan).
type El = { tag: string; id: string; classes: string[] }

const NAV: El = { tag: 'nav', id: '', classes: ['jump'] }
const SECTION: El = { tag: 'section', id: '', classes: [] }
const MAIN: El = { tag: 'main', id: 'main', classes: [] }
const LAYOUT: El = { tag: 'div', id: '', classes: ['layout'] }
const ROOT: El = { tag: 'div', id: 'root', classes: [] }
const BODY: El = { tag: 'body', id: '', classes: [] }
const HTML: El = { tag: 'html', id: '', classes: [] }

const CHAINS: { name: string; els: El[] }[] = [
  {
    name: 'the label (p.jump-label)',
    els: [
      { tag: 'p', id: 'model-profile-jump-label', classes: ['jump-label'] },
      NAV,
      SECTION,
      MAIN,
      LAYOUT,
      ROOT,
      BODY,
      HTML,
    ],
  },
  {
    name: 'the toggle (button.jump-toggle.secondary)',
    els: [
      {
        tag: 'button',
        id: 'model-profile-jump-toggle',
        classes: ['jump-toggle', 'secondary'],
      },
      NAV,
      SECTION,
      MAIN,
      LAYOUT,
      ROOT,
      BODY,
      HTML,
    ],
  },
  {
    name: 'the closed list (ul.collapsed)',
    els: [
      { tag: 'ul', id: 'model-profile-jump-list', classes: ['collapsed'] },
      NAV,
      SECTION,
      MAIN,
      LAYOUT,
      ROOT,
      BODY,
      HTML,
    ],
  },
]

// A selector part split into its compounds at the whitespace and the
// combinators OUTSIDE brackets and parentheses. Each compound carries the
// combinator before it.
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

// One compound: its tag, its id, its classes. An attribute selector, a
// pseudo-class or a pseudo-element is a constraint the chain (tag, id, classes)
// cannot carry, so a part that holds one is out of the scan (matchesChain
// rejects it): the sheet's `[hidden] { display: none !important }` would else
// win every `display` read of every element, and `[tabindex='-1']:focus` every
// `outline` read.
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
// the element; the compounds before it are walked up the chain.
const matchesChain = (els: El[], raw: string): boolean => {
  const chain = chainOf(raw)
  if (chain.length === 0) return false
  if (chain.some((c) => c.text.includes('[') || c.text.includes(':')))
    return false
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
// chain; null when none does.
const chainSpec = (sel: string, els: El[]): Spec | null => {
  let best: Spec | null = null
  for (const part of listItems(sel)) {
    if (!matchesChain(els, part)) continue
    const s = specOf(part)
    if (best === null || specGt(s, best)) best = s
  }
  return best
}

// The blocks whose window cannot include a narrow viewport (a page 400 px wide,
// tall, light, no forced colours), so a rule inside one is out of the scan.
const NOT_NARROW = [
  '@media (min-width:',
  '@media (prefers-color-scheme:',
  '@media (forced-colors:',
]

// Every rule that applies on a narrow viewport: the top level, and any block
// whose window can include it (the max-width and max-height blocks).
const narrowRules = (css: string): Rule[] => {
  const out = []
  for (const r of rulesOf(css)) {
    if (r.depth === 0) {
      out.push(r)
      continue
    }
    const wide = NOT_NARROW.some((needle) =>
      atRangesIn(css, needle).some(([a, e]) => r.at > a && r.at < e),
    )
    if (!wide) out.push(r)
  }
  return out
}

// The rule with the highest claim on this chain and property: highest
// specificity, and at a tie the last written (the cascade's own order).
const claim = (
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

// The chain of a name in CHAINS; a miss names the name.
const chainOfName = (name: string): El[] => {
  const found = CHAINS.filter((c) => c.name === name)
  if (found.length !== 1)
    throw new Error(`expected one chain named ${name}, found ${found.length}`)
  return found[0].els
}

/** The cascade of one element, property by property: every property the brief
 * names for it must come out with the brief's value, and the message names the
 * rule that wins today. */
function cascadeOf(chainName: string, want: [string, string][]): string[] {
  const els = chainOfName(chainName)
  const rules = narrowRules(CSS)
  const bad: string[] = []
  for (const [p, value] of want) {
    const won = claim(rules, els, p)
    if (won === null) {
      bad.push(
        `${chainName} ${p}: no rule that applies below 1280 px declares it`,
      )
      continue
    }
    if (prop(won.rule.body, p) !== value)
      bad.push(
        `${chainName} ${p}: ${where(won.rule.sel)} (${won.spec.join(',')}) wins with ` +
          `${prop(won.rule.body, p)}, and H10 wants ${value}`,
      )
  }
  return bad
}

// ── H10.6: the narrow look ───────────────────────────────────────────────

test('H10.6: at the top level, .jump-label is display: none with the --space-1 bottom margin, 700 and --fs-md; .jump-toggle is inline-flex, items centred, gap --space-2; and .jump-toggle[aria-expanded=true] .icon turns 180deg with no transition', () => {
  const label = bySel(topLevel(CSS), LABEL_SEL, 'H10.6 styles the label')
  expect(
    prop(label.body, 'display'),
    'the label is shown by the wide block',
  ).toBe('none')
  expect(
    prop(label.body, 'margin'),
    'the label sits tight above the toggle',
  ).toBe('0 0 var(--space-1)')
  expect(prop(label.body, 'font-weight'), 'the label reads as a heading').toBe(
    '700',
  )
  expect(prop(label.body, 'font-size'), 'at the body size').toBe('var(--fs-md)')

  const toggle = bySel(topLevel(CSS), TOGGLE_SEL, 'H10.6 styles the toggle')
  expect(prop(toggle.body, 'display'), 'the chevron sits on the text').toBe(
    'inline-flex',
  )
  expect(prop(toggle.body, 'align-items'), 'and is centred on it').toBe(
    'center',
  )
  expect(prop(toggle.body, 'gap'), 'with the small step between').toBe(
    'var(--space-2)',
  )

  const chevron = bySel(
    topLevel(CSS),
    CHEVRON_SEL,
    'H10.6 turns the chevron with the state',
  )
  expect(prop(chevron.body, 'rotate'), 'the turn shows the state').toBe(
    '180deg',
  )
  expect(
    prop(chevron.body, 'transition'),
    'section 0: no transition, so prefers-reduced-motion has nothing to stop',
  ).toBe('')
})

test('H10.6: a @media (max-width: 1279.98px) block hides the list by its class, .jump ul.collapsed: display: none, and nothing else (section 0: not hidden, so the 1280 px rail wins over the state)', () => {
  const rules = blockRules(CSS, NARROW, 'H10.6 hides the list below 1280 px')
  const rule = bySel(rules, COLLAPSED_SEL, 'the collapsed list')
  expect(prop(rule.body, 'display'), 'a closed list is gone').toBe('none')
  expect(
    rule.body
      .split(';')
      .filter((d) => d.includes(':'))
      .map((d) => d.slice(0, d.indexOf(':')).trim())
      .sort(),
    'one declaration and no other: the class only hides, it never restyles the list',
  ).toEqual(['display'])
})

test('H10.6 cascade: below 1280 px the label is hidden, the toggle is an inline-flex pill of --control-h with the .secondary border and radius, and a closed list is hidden: on each of those properties, over the top level and every block that can apply at that width, the rule with the highest claim declares what H10 wants', () => {
  const bad = [
    ...cascadeOf('the label (p.jump-label)', [
      ['display', 'none'],
      ['margin', '0 0 var(--space-1)'],
      ['font-weight', '700'],
      ['font-size', 'var(--fs-md)'],
    ]),
    ...cascadeOf('the toggle (button.jump-toggle.secondary)', [
      ['display', 'inline-flex'],
      ['align-items', 'center'],
      ['gap', 'var(--space-2)'],
      // H10.9: the 40 px pill is the global button min-height and the
      // .secondary border and radius, so those are what must win.
      ['min-height', 'var(--control-h)'],
      ['border', 'var(--border-w) solid var(--control)'],
      ['border-radius', 'var(--radius-pill)'],
    ]),
    ...cascadeOf('the closed list (ul.collapsed)', [['display', 'none']]),
  ]
  expect(
    bad,
    `the disclosure's own declarations lose the cascade on the narrow page:\n${bad.join('\n')}`,
  ).toEqual([])
})

// ── H10.7: the wide look ─────────────────────────────────────────────────

test('H10.7: inside the one @media (min-width: 1280px) block, .jump-label is display: block and .jump-toggle is display: none, so the rail shows the list whatever its class (H9b.1: the rail rules live in this block, and a second 1280 px block is refused)', () => {
  const rules = blockRules(CSS, WIDE, 'H9b keeps its rail rules in this block')
  expect(
    prop(
      bySel(rules, LABEL_SEL, 'the label shows from 1280 px').body,
      'display',
    ),
    'H10.7: the label is visible in the rail',
  ).toBe('block')
  expect(
    prop(
      bySel(rules, TOGGLE_SEL, 'the toggle is gone from 1280 px').body,
      'display',
    ),
    'H10.7: the list is always open beside the cards, so no disclosure',
  ).toBe('none')
})

test('H10.7: the label and the toggle display is declared at the top level and in the 1280 px block only, so a wider window never leaves the page with no label and no way to open the list', () => {
  const sel = [LABEL_SEL, TOGGLE_SEL]
  const outside = RULES.filter(
    (r) =>
      listItems(r.sel).length === 1 &&
      sel.includes(listItems(r.sel)[0]) &&
      prop(r.body, 'display') !== '' &&
      !(
        r.depth === 0 ||
        atRangesIn(CSS, WIDE).some(
          ([a, e]) => r.depth > 0 && r.at > a && r.at < e,
        )
      ),
  )
  expect(
    outside.map((r) => `${where(r.sel)} at depth ${r.depth}`),
    'H10.6 and H10.7 give these two rules and no other block may',
  ).toEqual([])
})

// ── H10.9: the ring is the global one ────────────────────────────────────

test('H10.9: the ring is the global :focus-visible outline, and no rule naming the jump list declares an outline, so the toggle keeps it (the ring drawn whole is for the UX pass)', () => {
  const ring = single(
    topLevel(CSS).filter((r) => listItems(r.sel)[0] === ':focus-visible'),
    'the global :focus-visible rule',
  )
  expect(prop(ring.body, 'outline'), 'H10.9: the global ring').toBe(
    'var(--ring-w) solid var(--accent)',
  )
  // The toggle carries no tabindex (pinned in HearthJumpToggle), so the
  // `[tabindex='-1']:focus { outline: none }` rule does not reach it. A sheet
  // that scoped a ring change to the jump list would take it away.
  const scoped = RULES.filter(
    (r) => prop(r.body, 'outline') !== '' && r.compact.includes('jump'),
  )
  expect(
    scoped.map((r) => `${where(r.sel)}: outline ${prop(r.body, 'outline')}`),
    'H10.9: nothing of the jump list overrides the ring',
  ).toEqual([])
})

// ── Self-probes of the scan (the teeth of the green) ─────────────────────

const SHEET = `
button { min-height: var(--control-h); border-radius: var(--radius); }
.secondary { border: var(--border-w) solid var(--control); border-radius: var(--radius-pill); }
.jump ul { display: block; }
.jump ul button { display: block; min-height: 32px; border: 0; }
.jump-label { display: none; }
@media (min-width: 1280px) {
  .jump-toggle { display: none; }
}
@media (max-width: 1279.98px) {
  .jump ul.collapsed { display: none; }
}
`

test('the scan counts specificity and source order, and a sheet that scopes the entry rule lets the disclosure win (the self-probe of the cascade test above)', () => {
  expect(specOf(TOGGLE_SEL), '.jump-toggle is 0,1,0').toEqual([0, 1, 0])
  expect(specOf('.jump button'), '.jump button is the 0,1,1 of H9a').toEqual([
    0, 1, 1,
  ])
  expect(specOf(COLLAPSED_SEL), '.jump ul.collapsed is 0,2,1').toEqual([
    0, 2, 1,
  ])

  const toggle = chainOfName('the toggle (button.jump-toggle.secondary)')
  // Today's shape: the index rule outweighs the pill, and the scan says so.
  const today = narrowRules(CSS)
  expect(
    prop(claim(today, toggle, 'min-height')!.rule.body, 'min-height'),
    'the finished sheet scopes the entry rule, so the 40px pill wins on the real sheet too',
  ).toBe('var(--control-h)')
  // A sheet that scopes the entry rule (here: to the buttons of the list, so
  // the disclosure is not one of them) lets the pill win on every property.
  const scoped = narrowRules(SHEET)
  const won = (p: string): string => {
    const c = claim(scoped, toggle, p)
    return c === null ? '' : `${where(c.rule.sel)}=${prop(c.rule.body, p)}`
  }
  expect(
    ['min-height', 'border', 'border-radius'].map(won),
    'the pill of .secondary wins once the entry rule no longer names the toggle',
  ).toEqual([
    'button=var(--control-h)',
    '.secondary=var(--border-w) solid var(--control)',
    '.secondary=var(--radius-pill)',
  ])
  // The narrow window: the collapsed rule hides the list, and the 1280 px
  // block is out of the scan, so it leaves the label and the toggle alone.
  const list = chainOfName('the closed list (ul.collapsed)')
  expect(
    prop(claim(scoped, list, 'display')!.rule.body, 'display'),
    '.jump ul.collapsed (0,2,1) hides the list',
  ).toBe('none')
  expect(
    claim(scoped, chainOfName('the label (p.jump-label)'), 'display')!.rule.sel,
    'the narrow scan never reads the 1280 px block',
  ).toBe('.jump-label')
  expect(
    claim(narrowRules(CSS), toggle, 'display')!.rule.sel,
    'on the finished sheet the disclosure owns its own display',
  ).toBe('.jump-toggle')
  // The strict matcher: a compound carrying an attribute, a pseudo-class or a
  // pseudo-element reaches no chain element, so `[hidden]` never wins a display
  // read and `[tabindex='-1']:focus` never wins an outline read.
  const label = chainOfName('the label (p.jump-label)')
  expect(
    claim(narrowRules(CSS), label, 'outline'),
    'the sheet outlines no chain element: its rules are all constrained',
  ).toBeNull()
  expect(
    claim(scoped, label, 'display')!.rule.sel,
    'the label display read is the sheet own rule, not the [hidden] one',
  ).toBe('.jump-label')
})
