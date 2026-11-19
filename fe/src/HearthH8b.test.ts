// fe/src/HearthH8b.test.ts
// H8b, the Lucide icons of the pages (fe/briefs/08-hearth-h8-icons.md), the
// stylesheet plane: H8b.4 the filter field, where the icon is placed inside it
// and the typed text starts after it, H8b.4's forced-colours rule for that
// icon, H8b.5 the rows that lay icon and text out (the run verbs, the back
// link, the notices, the notice icon), and H8b.6's side of the field error (the
// `.error-icon` rule gone, `.field-error > .icon` carrying its spacing).
//
// jsdom lays nothing out, so these are read as declarations the way
// HearthH2a.test.ts, HearthH5a.test.ts, HearthH5b.test.ts and
// HearthH8a.test.ts read them: comments dropped, the rules from the brace
// balance, the nesting from the balance before the selector, the specificity in
// three parts. The reader here is a local copy of theirs, because a test file
// imports no other test file.
//
// The rows of H8b.5 are read as the CASCADE RESOLVES them (`cascadeIn`), not as
// one named rule: the display of a button is what the sheet ends up giving the
// element, whatever rule carries it, and a regression can arrive as a later rule
// in a media block. Each test therefore runs its scan on doctored sheets first,
// which is what makes the pin on index.css mean anything: a scan that reports
// the doctored sheet is a scan with teeth. H8b.4 and H8b.5 name the rules, so
// those are pinned by their selector as well.
//
// H8b.1, H8b.2, H8b.3, H8b.6 (the DOM), H8b.7 and H8b.8 are in
// HearthIconsPages.test.tsx.
//
// OPEN, for the designer's UX pass (section 4): the icon lined up with the text
// beside it (no baseline jump), the search icon inside the field and not under
// typed text, the notices' icon on the first line, and forced colours: the icons
// taking the text's system colour, the search icon visible.
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
// counted from the braces before it). The cascade scans run on index.css AND on
// the short sheets the self-probes write, so the reader takes the text. RULES
// is the sheet's.
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
    // combinator (".notice .icon"), which compact cannot.
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

const TOKEN =
  /\[[^\]]*\]|::[a-zA-Z-]+|:[a-zA-Z-]+(?:\([^)]*\))?|\.[a-zA-Z-]+|#[a-zA-Z-]+|[a-zA-Z][a-zA-Z0-9-]*|[*+>~]|\s+/g

// A selector's specificity as the cascade counts it, in three parts: the ids
// (1,0,0), then the classes, the attribute selectors and the pseudo-classes
// (0,1,0), then the elements and the pseudo-elements (0,0,1). A functional
// pseudo-class counts as one (0,1,0) with its argument folded in.
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

// A selector part that carries `token` as a class, an id, an attribute or a
// pseudo-class: matched WHOLE, so `.iconology` is not `.icon`.
const hasToken = (raw: string, token: string): boolean => {
  const toks: string[] = raw.trim().match(TOKEN) ?? []
  return toks.includes(token)
}

// The name a block shows in a message: its selector, whitespace collapsed.
const where = (selector: string): string => selector.replace(/\s+/g, ' ').trim()

// The light token block's custom properties.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

const tokens = new Map<string, string>(
  customProps(topBy(':root', 'the light token block').body),
)

// ── The elements the pages render, and the cascade that reaches them ──────

// One element of a chain: its tag, its id, its classes (App.tsx and the page
// component as they render today).
type El = { tag: string; id: string; classes: string[] }

// Every page sits under `main#main`, under `div.layout`, under the mount point.
const SHELL: El[] = [
  { tag: 'main', id: 'main', classes: [] },
  { tag: 'div', id: '', classes: ['layout'] },
  { tag: 'div', id: 'root', classes: [] },
  { tag: 'body', id: '', classes: [] },
  { tag: 'html', id: '', classes: [] },
]

const under = (...els: El[]): El[] => [...els, ...SHELL]

// A run verb of the instance page, as InstancePage.tsx renders it.
const RUN_BUTTON = under(
  { tag: 'button', id: 'default-start', classes: ['secondary'] },
  { tag: 'div', id: '', classes: ['run-actions'] },
  { tag: 'div', id: '', classes: ['instance-header'] },
)

// The back link of a model profile, as ModelProfilePage.tsx renders it.
const BACK_LINK = under(
  { tag: 'a', id: '', classes: [] },
  { tag: 'p', id: '', classes: ['model-profile-back'] },
)

// A notice paragraph, as SignIn.tsx and ModelProfilePage.tsx render it, and the
// icon it holds.
const NOTICE = under({ tag: 'p', id: '', classes: ['notice'] })
const NOTICE_ICON = under({ tag: 'svg', id: '', classes: ['icon'] }, ...NOTICE)

// The filter field's row, its icon and its input, as ModelProfiles.tsx renders
// them.
const FILTER_ROW: El = {
  tag: 'div',
  id: '',
  classes: ['model-profiles-filter-row'],
}
const FILTER_BOX: El = {
  tag: 'div',
  id: '',
  classes: ['model-profiles-filter'],
}
const FILTER_ICON = under(
  { tag: 'svg', id: '', classes: ['icon'] },
  FILTER_ROW,
  FILTER_BOX,
)
const FILTER_INPUT = under(
  { tag: 'input', id: 'model-profiles-filter', classes: [] },
  FILTER_ROW,
  FILTER_BOX,
)

// The field error of a model profile, as Field.tsx renders it, and its icon.
const FIELD_ERROR = under({
  tag: 'p',
  id: 'model-profile-fit-error',
  classes: ['field-error'],
})
const FIELD_ERROR_ICON = under(
  { tag: 'svg', id: '', classes: ['icon'] },
  ...FIELD_ERROR,
)

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

// One compound: its tag, its id, its classes. An attribute selector, a
// pseudo-class or a pseudo-element is a constraint this test cannot evaluate
// against a chain (no chain element is `hidden`, none is `:hover`), so a
// compound carrying one never fits: such a rule is no winner of the cascade.
// Scans that report a rule by its selector do not use this and see every rule.
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

const fits = (c: El & { opaque: number }, el: El): boolean =>
  c.opaque === 0 &&
  (c.tag === '' || c.tag === el.tag) &&
  (c.id === '' || c.id === el.id) &&
  c.classes.every((k) => el.classes.includes(k))

// Does this selector part reach the element at the head of `chain`? Its LAST
// compound must fit that element, then the compounds before it are walked up the
// chain: a descendant may be any ancestor, a child must be the one, and a
// sibling combinator is not read, so a rule written with one is not a hit.
const matchesChain = (raw: string, chain: El[]): boolean => {
  const parts = chainOf(raw)
  if (parts.length === 0) return false
  if (!fits(compoundOf(parts[parts.length - 1].text), chain[0])) return false
  let i = 1
  for (let k = parts.length - 2; k >= 0; k--) {
    const c = compoundOf(parts[k].text)
    const comb = parts[k + 1].comb
    if (comb === '+' || comb === '~') return false
    if (comb === '>') {
      if (i >= chain.length || !fits(c, chain[i])) return false
      i++
      continue
    }
    while (i < chain.length && !fits(c, chain[i])) i++
    if (i >= chain.length) return false
    i++
  }
  return true
}

// The highest specificity among the parts of a selector list that reach the
// element; null when none does. The specificity of a list is that of the part
// that matches, not of the loudest part it also carries.
const specFor = (sel: string, chain: El[]): Spec | null => {
  let best: Spec | null = null
  for (const part of listItems(sel)) {
    if (!matchesChain(part, chain)) continue
    const s = specOf(part)
    if (best === null || specGt(s, best)) best = s
  }
  return best
}

// What the cascade ends up giving the element for one property: among every rule
// of the sheet that reaches it and declares the property, top level and inside a
// media block alike, the winner is the highest specificity, and a tie is decided
// by the order of the sheet. `pick` reads the value out of a block, so a
// shorthand can be folded in. Null: nothing declares it, which is a MISS for a
// rule the brief asks for, never a pass. A `!important` is not read, the sheet
// uses none.
const cascadeIn = (
  css: string,
  chain: El[],
  pick: (body: string) => string,
): { value: string; sel: string } | null => {
  let best: { value: string; sel: string; spec: Spec; at: number } | null = null
  for (const r of rulesOf(css)) {
    const value = pick(r.body)
    if (value === '') continue
    const spec = specFor(r.sel, chain)
    if (spec === null) continue
    if (
      best === null ||
      specGt(spec, best.spec) ||
      (specEq(spec, best.spec) && r.at > best.at)
    )
      best = { value, sel: r.sel, spec, at: r.at }
  }
  return best === null ? null : { value: best.value, sel: best.sel }
}

// The value the sheet ends up giving `chain` for `name`, '' when nothing declares
// it. A `padding: …` shorthand is folded into its left side: a shorthand written
// LATER beats an earlier longhand at the same specificity, which is exactly how a
// placement of the icon is undone, so the scan reads the effective padding and not
// the longhand alone.
const sideOf =
  (name: string) =>
  (body: string): string => {
    const long = prop(body, name)
    if (long !== '') return long
    if (name !== 'padding-left') return ''
    const parts = prop(body, 'padding')
      .split(/\s+/)
      .filter((p) => p !== '')
    if (parts.length === 0) return ''
    // top / top right / top right bottom / top right bottom left.
    return parts.length === 4
      ? parts[3]!
      : parts.length === 1
        ? parts[0]!
        : parts[1]!
  }
const value = (css: string, chain: El[], name: string): string =>
  cascadeIn(css, chain, sideOf(name))?.value ?? ''

// The calc values are compared with their spaces dropped: the brief spells
// `calc((1lh - 1.25em) / 2)` and a formatter is free to close that up.
const flatValue = (v: string): string => v.replace(/\s+/g, '')

// ── H8b.4: the filter field, the icon inside it ───────────────────────────

test('H8b.4: the filter row is the positioned box and its icon is placed inside the field, the row keeps the layout of 2b, and the input ends up padded past the icon (the self-probes are the teeth of the cascade)', () => {
  const row = topBy(tight('.model-profiles-filter-row'), 'the filter row (2b)')
  expect(
    declNames(row.body),
    'the three of 2b plus the position the icon needs, and nothing else',
  ).toEqual(['align-items', 'display', 'gap', 'position'])
  expect(
    prop(row.body, 'position'),
    'the positioned box the absolute icon sits in',
  ).toBe('relative')
  expect(prop(row.body, 'display'), 'the row of 2b is kept').toBe('flex')
  expect(prop(row.body, 'align-items'), 'and kept').toBe('center')
  expect(prop(row.body, 'gap'), 'and kept').toBe('var(--space-2)')

  const glyph = topBy(
    tight('.model-profiles-filter-row > .icon'),
    'the icon inside the filter field (H8b.4)',
  )
  expect(
    declNames(glyph.body),
    'the four the brief lists, and no more',
  ).toEqual(['color', 'left', 'pointer-events', 'position'])
  expect(
    prop(glyph.body, 'position'),
    'out of the row box, into the field',
  ).toBe('absolute')
  expect(prop(glyph.body, 'left'), 'at the field padding token').toBe(
    'var(--space-3)',
  )
  expect(
    prop(glyph.body, 'pointer-events'),
    'a click on the icon reaches the field, not the drawing',
  ).toBe('none')
  expect(prop(glyph.body, 'color'), 'the muted tint of a search field').toBe(
    'var(--muted)',
  )
  expect(
    tokens.has('--muted'),
    'the tint is a token, so both themes and the forced-colours rule reach it',
  ).toBe(true)
  expect(
    specGt(specOf('.model-profiles-filter-row > .icon'), specOf('.icon')),
    '0,2,0 over 0,1,0: the placement wins on the icon it names, and the `.icon` box of H8a.4 stays in force on every other icon',
  ).toBe(true)
  expect(
    specFor('.model-profiles-filter-row > .icon', NOTICE_ICON),
    'the child combinator is read: a notice icon is inside `.notice`, not inside a filter row, so the tint and the placement reach the field icon only',
  ).toBeNull()
  for (const name of ['position', 'left', 'pointer-events'])
    expect(
      value(CSS, FILTER_ICON, name),
      `the cascade: the row's icon ends up with ${name}`,
    ).toBe(prop(glyph.body, name))
  expect(
    value(CSS, NOTICE_ICON, 'position'),
    'a notice icon gets no absolute placement from this rule',
  ).toBe('')

  // The cascade, on sheets written here: the shorthand trap, and a scan that
  // reports a rule that overrides.
  expect(
    value(
      '.model-profiles-filter-row input { padding: 1px 2px }',
      FILTER_INPUT,
      'padding-left',
    ),
    'the shorthand has a left side: the scan reads the effective padding, not the longhand alone',
  ).toBe('2px')
  expect(
    value(
      '.model-profiles-filter-row input { padding-left: 9px } .model-profiles-filter-row input { padding: 4px 5px }',
      FILTER_INPUT,
      'padding-left',
    ),
    'a shorthand written AFTER the longhand at the same specificity wins it: this is how a placement of the icon is undone, and the scan reports the shorthand',
  ).toBe('5px')
  expect(
    value(
      '.model-profiles-filter-row input { padding-left: 9px } input { padding: 4px 5px }',
      FILTER_INPUT,
      'padding-left',
    ),
    'the shared `input` rule is (0,0,1) and loses to the field rule at (0,1,1), written before it',
  ).toBe('9px')
  expect(
    value(
      '.model-profiles-filter label { padding-left: 9px }',
      FILTER_INPUT,
      'padding-left',
    ),
    'a rule about the label is no rule about the field',
  ).toBe('')
  expect(
    value(CSS, FIELD_ERROR, 'padding-left'),
    'nothing declared is no value: a rule the brief asks for, missing, is a miss',
  ).toBe('')

  // The real sheet.
  expect(
    flatValue(value(CSS, FILTER_INPUT, 'padding-left')),
    'the typed text starts past the icon: the field padding, the icon box, and a space between',
  ).toBe('calc(var(--space-3)+1.25em+var(--space-2))')
  expect(
    prop(topBy(tight('.icon'), 'the .icon box (H8a.4)').body, 'width'),
    'the calc adds the icon box, so the two must agree: 1.25em is H8a.4',
  ).toBe('1.25em')
  expect(
    tokens.get('--space-3'),
    'the field padding the calc starts from',
  ).toBe('12px')
})

// ── H8b.4: forced colours, the tint given back ────────────────────────────

const FORCED = '@media (forced-colors: active)'

// Every rule of a CSS text inside a forced-colours block that reaches `.icon`,
// as the colour declaration it carries.
const forcedIconIn = (css: string): string[] =>
  insideIn(css, FORCED, '.icon')
    .filter((r) => listItems(r.sel).some((raw) => hasToken(raw, '.icon')))
    .map((r) => `${where(r.sel)}: color: ${prop(r.body, 'color')}`)
    .filter((d) => !d.endsWith(': color: '))

test('H8b.4: the --muted tint of the filter icon is a colour of its own, so a forced-colours block gives it color: inherit, as .mark (the self-probes are the teeth of the scan)', () => {
  // The scan first, on sheets written here.
  expect(
    forcedIconIn(
      `${FORCED} { .model-profiles-filter-row > .icon { color: inherit } }`,
    ),
    'the shape the brief asks for, read back',
  ).toEqual(['.model-profiles-filter-row > .icon: color: inherit'])
  expect(
    forcedIconIn(`${FORCED} { .notice > .icon { color: CanvasText } }`),
    'the inside of the block is read, and a rule for another icon is listed as that',
  ).toEqual(['.notice > .icon: color: CanvasText'])
  expect(
    forcedIconIn(`${FORCED} { .iconology { color: inherit } }`),
    'a class that only starts like `.icon` is no hit',
  ).toEqual([])
  expect(
    forcedIconIn(`${FORCED} { .icon { border-color: GrayText } }`),
    'a rule that sets no colour is not listed',
  ).toEqual([])
  expect(
    forcedIconIn('.model-profiles-filter-row > .icon { color: inherit }'),
    'the rule OUTSIDE the block is not what this pin asks for',
  ).toEqual([])

  // The real sheet: one rule, reaching the filter icon, giving `color: inherit`.
  const hits = inside(FORCED, '.icon').filter((r) =>
    listItems(r.sel).some((raw) => hasToken(raw, '.icon')),
  )
  expect(
    forcedIconIn(CSS),
    'exactly one forced-colours rule reaches `.icon` with a colour',
  ).toEqual(['.model-profiles-filter-row > .icon: color: inherit'])
  expect(hits.length, 'and it is one rule').toBe(1)
  expect(
    prop(hits[0]!.body, 'color'),
    'inherit: an svg keeps its author colour in forced colours (the UA preserve-parent-color), so the --muted tint has to be handed back to the system colour, as the mark of H3.10',
  ).toBe('inherit')
  expect(
    specFor(hits[0]!.sel, FILTER_ICON),
    'and the rule reaches the filter icon: it is the one H8b.4 asks for, not a rule about another icon',
  ).not.toBeNull()
})

// ── H8b.5: the hosts lay icon and text out ────────────────────────────────

test('H8b.5: the run verbs and the back link are centred inline-flex rows at the space-2 gap, the notices are flex rows at flex-start with the icon centred on the first line, all read as the cascade resolves them (the self-probes are the teeth of the scan)', () => {
  // The scan, on sheets written here.
  expect(
    value(
      '.run-actions button { display: inline-flex } .instance-header .run-actions button { display: block }',
      RUN_BUTTON,
      'display',
    ),
    'a later rule with a higher specificity wins: a regression of that shape is reported, not hidden',
  ).toBe('block')
  expect(
    value(
      'button { display: block } .run-actions button { display: inline-flex }',
      RUN_BUTTON,
      'display',
    ),
    'the shared `button` rule is (0,0,1): it loses to the row at (0,1,1), which is why the row of H8b.5 is free to carry the display of a control',
  ).toBe('inline-flex')
  expect(
    value(
      '.run-actions button { display: inline-flex } .run-actions > button { display: grid }',
      RUN_BUTTON,
      'display',
    ),
    'a tie at (0,1,1) is decided by the order of the sheet: the later rule wins, so the row has to stay above what overrides it',
  ).toBe('grid')
  expect(
    value(`${FORCED} { .notice { display: block } }`, NOTICE, 'display'),
    'a rule inside a media block is read, not skipped: the media query adds no specificity',
  ).toBe('block')
  expect(
    value('.markdown { display: flex }', RUN_BUTTON, 'display'),
    'a rule reaching nothing of the page is not a value',
  ).toBe('')

  // The real sheet: the two rows of verbs and links.
  for (const [chain, what] of [
    [RUN_BUTTON, 'the run verb button'],
    [BACK_LINK, 'the back link'],
  ] as [El[], string][]) {
    expect(
      value(CSS, chain, 'display'),
      `${what}: inline-flex, so the icon and the words sit in one row`,
    ).toBe('inline-flex')
    expect(
      value(CSS, chain, 'align-items'),
      `${what}: the icon centred on the words it sits beside`,
    ).toBe('center')
    expect(
      value(CSS, chain, 'gap'),
      `${what}: the gap of the tokens, not a literal`,
    ).toBe('var(--space-2)')
  }
  expect(tokens.get('--space-2'), 'the gap token is 8 px').toBe('8px')

  // The notices: a column of icon and text, the icon on the first line.
  expect(
    value(CSS, NOTICE, 'display'),
    'the notice: a flex row, so the icon sits beside the text',
  ).toBe('flex')
  expect(
    value(CSS, NOTICE, 'align-items'),
    'flex-start, not center: a notice wraps to several lines and the icon belongs to the first',
  ).toBe('flex-start')
  expect(value(CSS, NOTICE, 'gap'), 'the same gap').toBe('var(--space-2)')
  expect(
    flatValue(value(CSS, NOTICE_ICON, 'margin-top')),
    'the icon centred on the first line: half the line box less half the icon box',
  ).toBe('calc((1lh-1.25em)/2)')
  expect(
    specGt(specOf('.notice > .icon'), specOf('.notice')),
    '0,2,0 over 0,1,0: the icon rule wins on the icon, so the notice keeps its own box',
  ).toBe(true)

  // Section 5: the boxes these rules sit on do not move.
  const notice = topBy(tight('.notice'), 'the notice box (D5.2)')
  expect(prop(notice.body, 'padding'), 'its padding kept').toBe(
    'var(--space-2) var(--space-3)',
  )
  expect(prop(notice.body, 'border'), 'its border kept').toBe(
    '1px solid var(--control)',
  )
  expect(prop(notice.body, 'background'), 'its background kept').toBe(
    'var(--surface)',
  )
  const actions = topBy(tight('.run-actions'), 'the row of the verbs (1c)')
  expect(prop(actions.body, 'display'), 'the row of the verbs is kept').toBe(
    'flex',
  )
  expect(prop(actions.body, 'flex-wrap'), 'it still wraps').toBe('wrap')
  expect(prop(actions.body, 'margin-bottom'), 'and keeps its space below').toBe(
    'var(--space-2)',
  )
})

// ── H8b.6: the field error, the CSS side ──────────────────────────────────

test('H8b.6: the .error-icon rule is gone and `.field-error > .icon` carries its spacing, lowered to -0.25em for the 24 grid, and the field error stays in --danger (the self-probe is the teeth of the absence)', () => {
  // The scan, on sheets written here.
  const named = (css: string): string[] =>
    rulesOf(css)
      .filter((r) =>
        listItems(r.sel).some((raw) => hasToken(raw, '.error-icon')),
      )
      .map((r) => where(r.sel))
  expect(
    named('.error-icon { margin-right: var(--space-1) }'),
    'the rule the brief removes, reported when it is there',
  ).toEqual(['.error-icon'])
  expect(
    named('.field-error > .icon { margin-right: var(--space-1) }'),
    'the replacement is not the old class',
  ).toEqual([])
  expect(
    named('@media (forced-colors: active) { .error-icon { color: inherit } }'),
    'a rule inside a media block counts too: the class must be gone from the sheet',
  ).toEqual(['.error-icon'])

  // The real sheet: the class is used by no rule, at any nesting.
  expect(
    named(CSS),
    'H8b.6: the `.error-icon` rule goes with the svg it sized, it is not left to match nothing',
  ).toEqual([])

  const icon = topBy(
    tight('.field-error > .icon'),
    'the spacing of the icon of the field error (H8b.6)',
  )
  expect(
    declNames(icon.body),
    'the two properties the brief lists, and no more: the spacing of the old rule, and the drop',
  ).toEqual(['margin-right', 'vertical-align'])
  expect(prop(icon.body, 'margin-right'), 'the space of the old rule').toBe(
    'var(--space-1)',
  )
  expect(
    flatValue(prop(icon.body, 'vertical-align')),
    '-0.25em, not the -0.2em of the 16 box: the icon is 1.25em of the body-small text now',
  ).toBe('-0.25em')
  expect(
    value(CSS, FIELD_ERROR_ICON, 'margin-right'),
    'the cascade: the rule reaches the icon inside the field error',
  ).toBe('var(--space-1)')
  expect(
    value(CSS, NOTICE_ICON, 'margin-right'),
    'and it reaches only an icon inside a field error',
  ).toBe('')

  // Section 5: the field error itself does not move, and stays in --danger.
  const err = topBy(tight('.field-error'), 'the field error box (DESIGN 5.2)')
  expect(prop(err.body, 'margin'), 'its margins kept').toBe(
    'var(--space-1) 0 0',
  )
  expect(prop(err.body, 'font-size'), 'its size kept').toBe('var(--fs-sm)')
  expect(
    value(CSS, FIELD_ERROR, 'color'),
    'the field error of 128 is still shown in --danger',
  ).toBe('var(--danger)')
})
