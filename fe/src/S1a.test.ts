// S1a.12 of fe/briefs/09-signin-only.md, the part jsdom cannot see: the
// stylesheet (the banner's `a.session` selectors go, `main` spans the whole
// `.layout` row when it is its only child) and the code that goes with it
// (Overview and Instances lose the `signedIn` prop, SessionBar the `current`
// one, the banner's Sign in text is no longer shown), plus the keys the brief
// says stay and the three texts it rewords. The DOM part of the criterion is in
// SignInOnly.test.tsx. index.css is read the way HearthH2a.test.ts and
// HearthTokens.test.ts do (comments dropped, the same rule regex, the same
// nesting depth, prop() the last value of a property in a block); none of them
// exports a helper, so the reader here is a small local copy of theirs.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'
import strings from './strings/en_US.json'

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const src = (file: string): string =>
  readFileSync(join(import.meta.dirname, file), 'utf8')

type Rule = { sel: string; flat: string; body: string; at: number }

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1]!.trim(),
  // Runs of whitespace become one space, so a needle can name a descendant
  // combinator (".layout > main"), which a compact one cannot.
  flat: m[1]!.trim().replace(/\s+/g, ' '),
  body: m[2]!,
  at: m.index ?? 0,
}))

// The selector list of a rule: its comma-separated parts, as written.
const listItems = (sel: string): string[] =>
  sel
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s !== '')

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

/** The span of the block whose prelude is `prelude`, as `[open, close]`. */
function blockSpan(prelude: string): [number, number] {
  const i = CSS.indexOf(prelude)
  expect(i, `index.css has no ${prelude} block`).toBeGreaterThan(-1)
  const open = CSS.indexOf('{', i)
  let depth = 0
  for (let j = open; j < CSS.length; j++) {
    if (CSS[j] === '{') depth++
    else if (CSS[j] === '}') {
      depth--
      if (depth === 0) return [open, j]
    }
  }
  throw new Error(`${prelude} is not closed`)
}

// ── The a.session selectors are gone ──────────────────────────────────────

test('09 S1a.12: no rule of index.css reaches a.session, at the top level or in a media block (the banner pill is gone, S1a.4)', () => {
  const named = RULES.filter((r) =>
    listItems(r.sel).some(
      (item) =>
        item === 'a.session' || item === "a.session[aria-current='page']",
    ),
  )
  expect(
    named.map((r) => r.flat),
    'the selectors of the banner Sign in pill, in the rules that named them',
  ).toEqual([])
  expect(
    [...CSS.matchAll(/a\.session/g)].length,
    'and nothing at all in the stylesheet names a.session',
  ).toBe(0)
})

// ── main spans the row when it is the only child ──────────────────────────

test('09 S1a.12: inside the min-width: 960px block, .layout > main:only-child spans the whole row: grid-column: 1 / -1 (signed out there is no nav column)', () => {
  const [open, close] = blockSpan('@media (min-width: 960px)')
  const wide = RULES.filter((r) => r.at > open && r.at < close)
  const spans = wide.filter((r) => r.flat === '.layout > main:only-child')
  expect(spans, 'the one rule that widens a page with no nav').toHaveLength(1)
  expect(prop(spans[0]!.body, 'grid-column'), 'the whole row').toBe('1 / -1')
  // The rule is only useful beside the grid the same block gives .layout.
  const layout = wide.filter((r) => r.flat === '.layout')
  expect(layout, 'the .layout rule of the wide block').toHaveLength(1)
  expect(
    prop(layout[0]!.body, 'grid-template-columns'),
    'the nav column that main must not fall into',
  ).toContain('224px')
})

// ── The code that goes: the signedIn prop, the current prop, the pill ─────

test('09 S1a.12: Overview and Instances carry no signedIn prop, and App passes it to no page (the Overview renders only signed in)', () => {
  for (const file of ['Overview.tsx', 'Instances.tsx'])
    expect(
      src(file),
      `${file}: the signedIn prop is gone, so every tile sends its boot read`,
    ).not.toContain('signedIn')
  expect(src('App.tsx'), 'no page is handed a session state').not.toContain(
    'signedIn={',
  )
})

test('09 S1a.12: App renders SessionBar with no current prop, and the banner shows no Sign in text (the pill of S1a.4 is gone)', () => {
  const app = src('App.tsx')
  const bar = /<SessionBar\b[\s\S]*?\/>/.exec(app)
  expect(bar, 'the shell still renders SessionBar').toBeTruthy()
  expect(bar![0], 'the current prop is gone with the pill').not.toContain(
    'current',
  )
  expect(app, 'the banner has no Sign in link to name').not.toContain(
    'session-sign-in',
  )
})

// ── The strings of the brief's table ──────────────────────────────────────

test('09 S1a.12: no key is deleted: session-sign-in, signin-required and not-found-home stay in en_US.json', () => {
  const table: Record<string, string> = strings
  const gone = [
    'session-sign-in',
    'signin-required',
    'not-found-home',
    'signin-title',
    'loading',
  ].filter((key) => table[key] === undefined)
  expect(gone, 'keys the brief keeps').toEqual([])
})

test('09 S1a.12: the three texts drop their clause that promised an Overview to the signed out', () => {
  const table: Record<string, string> = strings
  expect(table['api-session-unavailable'], 'the brief section 1 table').toBe(
    'Signing in is not available on this server right now.',
  )
  expect(
    table['api-session-not-administrator'],
    'the brief section 1 table',
  ).toBe(
    'Only members of the pai group can sign in, and this account is not one. Ask whoever administers this computer to add it.',
  )
  expect(table['session-revoked'], 'the brief section 1 table').toBe(
    'You have been signed out: this account is no longer a member of the pai group. Ask whoever administers this computer to add it again.',
  )
  for (const key of [
    'api-session-unavailable',
    'api-session-not-administrator',
    'session-revoked',
  ])
    expect(
      table[key]!,
      `${key}: nothing promises the Overview any more`,
    ).not.toContain('overview')
})
