// H5a, the banner of the Hearth shell (fe/briefs/07-hearth-h5-shell.md, section 1,
// criteria H5a.2 to H5a.7): a restyle of the shell's rules, CSS only, so these
// tests pin the stylesheet text. jsdom does no layout, so index.css is read the way
// HearthH2a.test.ts, HearthH4.test.ts and DisabledLook.test.ts do (comments
// dropped, the same rule regex, the same nesting depth, prop() the last value of a
// property in a block); those files export no helper, so the reader is a small
// local copy. The DOM of the tools group (H5a.1) is in HearthBanner.test.tsx, and
// how the banner looks on the paper is the designer's UX pass.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'
import strings from './strings/en_US.json'
import { EN } from './test/bff.ts'

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
  // combinator (".session > span"), which compact cannot.
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

// Every declaration of a block, `name: value`, in order: a block that must hold
// exactly the declarations of another is compared as a sorted list of these.
const declarations = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.trim())
    .filter((d) => d.includes(':'))
    .map((d) => d.replace(/\s+/g, ' '))

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

const flatBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.flat === sel),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
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

/** Every top-level at-rule block whose prelude contains `needle`. */
const atRanges = (needle: string): [number, number][] => {
  const found: [number, number][] = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(needle, from)
    if (at < 0) return found
    const end = rangeEnd(at)
    found.push([at, end])
    from = end + 1
  }
}

/** The rules inside the at-rule block that starts at `open`. */
const inside = (range: [number, number]): Rule[] =>
  RULES.filter((r) => r.depth > 0 && r.at > range[0] && r.at < range[1])

/** The rules of `ranges`, for a media block that appears more than once. */
const insideAll = (ranges: [number, number][]): Rule[] => ranges.flatMap(inside)

/** The one block written with this at-rule header; a miss names the header. */
const oneAt = (needle: string): [number, number] => {
  const ranges = atRanges(needle)
  if (ranges.length !== 1)
    throw new Error(
      `expected exactly one ${needle} block, found ${ranges.length}`,
    )
  return ranges[0]
}

// The strings file (H5a.7 adds and changes no key).
const S = strings as Record<string, string>

// ── H5a.2: the banner sits on the paper ─────────────────────────────────

test('H5a.2: .banner is transparent with no border-bottom, padded --space-4 --space-5, and a wrapping flex row centred at the --space-2 --space-3 gap', () => {
  const banner = topBy('.banner', 'the .banner rule')
  expect(
    prop(banner.body, 'background'),
    'transparent: the shell sits on the page and the content cards lift off it (brief 04 1.2)',
  ).toBe('transparent')
  expect(
    ['', '0'].includes(prop(banner.body, 'border-bottom')),
    'the divider rule is gone: no border-bottom declaration, or border-bottom: 0',
  ).toBe(true)
  expect(prop(banner.body, 'padding'), 'the wide padding of the brief').toBe(
    'var(--space-4) var(--space-5)',
  )
  expect(prop(banner.body, 'display'), 'a flex row').toBe('flex')
  expect(prop(banner.body, 'flex-wrap'), 'that wraps').toBe('wrap')
  expect(prop(banner.body, 'align-items'), 'centred').toBe('center')
  expect(
    prop(banner.body, 'gap'),
    'the row gap is --space-2, the column gap --space-3',
  ).toBe('var(--space-2) var(--space-3)')
})

// ── H5a.3: the tools group carries the push to the end ──────────────────

test('H5a.3: .banner-tools is a top-level wrapping flex row centred at --space-2 with margin-left: auto, and the .session rule gives up margin-left and keeps its flex, wrap and gap', () => {
  const tools = topBy('.banner-tools', 'the .banner-tools rule')
  expect(prop(tools.body, 'display'), 'a flex row').toBe('flex')
  expect(prop(tools.body, 'flex-wrap'), 'that wraps as one group').toBe('wrap')
  expect(prop(tools.body, 'align-items'), 'centred').toBe('center')
  expect(prop(tools.body, 'gap'), 'at the small gap').toBe('var(--space-2)')
  expect(
    prop(tools.body, 'margin-left'),
    'pushed to the end, so the tools stay right-aligned at every width',
  ).toBe('auto')
  const session = topBy('.session', 'the .session rule')
  expect(
    ['', '0'].includes(prop(session.body, 'margin-left')),
    'no margin-left declaration on .session, or 0: the tools group carries the push now',
  ).toBe(true)
  expect(prop(session.body, 'display'), '.session keeps its flex').toBe('flex')
  expect(prop(session.body, 'flex-wrap'), 'its wrap').toBe('wrap')
  expect(prop(session.body, 'align-items'), 'its centring').toBe('center')
  expect(prop(session.body, 'gap'), 'and its gap, as 46b left them').toBe(
    'var(--space-2) var(--space-3)',
  )
  expect(
    prop(flatBy('.session > span', 'the 46b rule').body, 'overflow-wrap'),
    'the 46b rule that breaks a long user name inside the banner stays',
  ).toBe('anywhere')
})

// ── H5a.4: the Sign in link ─────────────────────────────────────────────

test('H5a.4: a.session is a pill: --radius-pill, padded --space-1 --space-4, at least 32px, an inline-flex row centred', () => {
  const link = topBy('a.session', 'the a.session rule (the Sign in link)')
  expect(
    prop(link.body, 'border-radius'),
    'a pill, like the .primary and .secondary controls',
  ).toBe('var(--radius-pill)')
  expect(prop(link.body, 'padding'), 'the pill padding').toBe(
    'var(--space-1) var(--space-4)',
  )
  expect(
    prop(link.body, 'min-height'),
    '32 px: above the 24 px target of 2.5.8, under the 40 px of a control',
  ).toBe('32px')
  expect(prop(link.body, 'display'), 'a row of its own').toBe('inline-flex')
  expect(prop(link.body, 'align-items'), 'centred').toBe('center')
})

// ── H5a.5: below 480 px, the icon-only button and the hidden name ───────

test('H5a.5: in one @media (max-width: 479.98px) block, the rule of .theme-switch-label and .session > span carries the declarations of .visually-hidden, .theme-switch is a --control-h square padded --space-1 --space-2, and the 46b rule outside is unchanged', () => {
  const block = oneAt('@media (max-width: 479.98px)')
  const narrow = inside(block)
  const hidden = single(
    narrow.filter((r) => {
      const items = listItems(r.sel)
      return (
        items.length === 2 &&
        items.includes('.theme-switch-label') &&
        items.includes('.session > span')
      )
    }),
    'the block rule whose selector list is exactly .theme-switch-label and .session > span',
  )
  const vis = topBy('.visually-hidden', 'the .visually-hidden rule')
  expect(
    declarations(hidden.body).slice().sort(),
    'the same declarations as .visually-hidden, in a media rule: a class toggled by width would need script',
  ).toEqual(declarations(vis.body).slice().sort())
  expect(
    prop(hidden.body, 'position'),
    'so the name of the button and the text of the banner stay, in the DOM and named',
  ).toBe('absolute')
  const sw = single(
    narrow.filter((r) => r.compact === '.theme-switch'),
    'the .theme-switch rule inside the block',
  )
  expect(
    prop(sw.body, 'min-width'),
    'the icon-only button keeps the 40 px square of 2.5.8',
  ).toBe('var(--control-h)')
  expect(
    prop(sw.body, 'padding'),
    'the padding that leaves the icon alone in it',
  ).toBe('var(--space-1) var(--space-2)')
  const span = flatBy('.session > span', 'the 46b rule')
  expect(
    span.depth,
    'the 46b rule stays a top-level rule, outside the block',
  ).toBe(0)
  expect(
    declarations(span.body),
    'and it keeps exactly its one declaration: the hiding rule is the one in the block, not this one',
  ).toEqual(['overflow-wrap: anywhere'])
})

// ── H5a.6: the 320 px gutter ────────────────────────────────────────────

test('H5a.6: inside the @media (max-width: 959.98px) block .banner takes padding --space-3 --space-4, and the rule that hides the closed nav stays', () => {
  const block = oneAt('@media (max-width: 959.98px)')
  const rule = single(
    inside(block).filter((r) => r.compact === '.banner'),
    'the .banner rule inside the narrow block',
  )
  expect(
    prop(rule.body, 'padding'),
    'the 16 px gutter of the page, so the mark, the name and the tools fit at 320 px (1.4.10)',
  ).toBe('var(--space-3) var(--space-4)')
  const closed = single(
    inside(block).filter((r) => r.compact === '.nav:not(.open)'),
    'the rule that hides the closed nav',
  )
  expect(
    prop(closed.body, 'display'),
    'the 320 px rule of section 5: the nav still collapses behind the Menu button',
  ).toBe('none')
})

// ── H5a.7: nothing else moves ───────────────────────────────────────────

test('H5a.7: the nav rules are untouched (H5b takes them) and no strings key is added or changed', () => {
  const nav = topBy('.nav', 'the .nav rule')
  expect(
    prop(nav.body, 'background'),
    'the nav keeps --surface: H5a is the banner only',
  ).toBe('var(--surface)')
  expect(prop(nav.body, 'padding'), 'and today padding').toBe('var(--space-4)')
  expect(prop(nav.body, 'border-bottom'), 'and its divider').toBe(
    '1px solid var(--control)',
  )
  const link = topBy('.nava', 'the .nav a rule')
  expect(prop(link.body, 'display'), 'a block link, not a pill').toBe('block')
  expect(prop(link.body, 'padding'), 'today padding').toBe(
    'var(--space-1) var(--space-3)',
  )
  expect(prop(link.body, 'border-radius'), 'today --radius').toBe(
    'var(--radius)',
  )
  expect(prop(link.body, 'color'), 'the text colour').toBe('var(--text)')
  expect(prop(link.body, 'text-decoration'), 'no underline').toBe('none')
  expect(prop(link.body, 'overflow-wrap'), 'and the long id break').toBe(
    'anywhere',
  )
  expect(
    prop(topBy('.nava:hover', 'the .nav a:hover rule').body, 'text-decoration'),
    'the hover underline stays (section 0: links, not controls)',
  ).toBe('underline')
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
    'the selector list of DESIGN.md 2.3, unchanged',
  ).toEqual([
    ".nav a[aria-current='page']",
    ".nav a[aria-current='true']",
    "a.session[aria-current='page']",
  ])
  expect(
    prop(current.body, 'box-shadow'),
    'the inset bar stays until H5b.3 replaces it with the --accent-soft pill',
  ).toBe('inset 3px 0 0 var(--accent)')
  expect(prop(current.body, 'background'), 'and no pill is added in H5a').toBe(
    '',
  )
  expect(prop(current.body, 'color'), 'the accent text').toBe('var(--accent)')
  expect(prop(current.body, 'font-weight'), 'and the weight').toBe('600')
  const label = topBy('.nav-label', 'the .nav-label rule')
  expect(prop(label.body, 'font-size'), 'the label keeps --fs-sm').toBe(
    'var(--fs-sm)',
  )
  expect(prop(label.body, 'margin'), 'and its margin').toBe(
    'var(--space-3) 0 var(--space-1)',
  )
  expect(prop(label.body, 'padding'), 'and its padding').toBe(
    '0 var(--space-3)',
  )
  const wide = single(
    insideAll(atRanges('@media (min-width: 960px)')).filter(
      (r) => r.compact === '.nav',
    ),
    'the .nav rule inside the @media (min-width: 960px) block',
  )
  expect(
    prop(wide.body, 'border-bottom'),
    'the wide nav keeps border-bottom: 0',
  ).toBe('0')
  expect(
    prop(wide.body, 'border-right'),
    'and the column divider, which H5b.1 drops',
  ).toBe('1px solid var(--control)')
  // No key: H5a adds and changes no text (brief section 0), and the banner
  // names are the ones H4.2 pinned.
  expect(
    Object.keys(S).filter((k) => /banner|tools|theme-switch/i.test(k)),
    'no key names the banner, the tools group or the theme switch label',
  ).toEqual([])
  expect(S['app-name'], 'the home link text').toBe('Private AI Framework')
  expect(S['nav-menu'], 'the Menu button text').toBe('Menu')
  expect(S['theme-dark'], 'the theme button text').toBe('Dark theme')
  expect(S['session-sign-in'], 'the Sign in text').toBe('Sign in')
  expect(S['session-sign-out'], 'the Sign out text').toBe('Sign out')
  expect(S['session-signed-in-as'], 'the signed-in text').toBe(
    'Signed in as {0}',
  )
  for (const k of [
    'app-name',
    'nav-menu',
    'theme-dark',
    'session-sign-in',
    'session-sign-out',
    'session-signed-in-as',
  ])
    expect(EN[k], `the test mirror of ${k}`).toBe(S[k])
})
