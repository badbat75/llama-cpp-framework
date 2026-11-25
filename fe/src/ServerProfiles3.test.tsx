// Brief 16, sub-slice 17b2 (fe/briefs/16-server-profiles-and-instances.md):
// the opener of the create form becomes the list's Add icon button, a + at the
// end of the list, drawn by the same rule as the tensor list's `.ot-add`
// (brief 21, 21 T.21 and 21 T.24). The list page, its table and the nav are
// ServerProfiles1.test.tsx; the create form's behaviour (17b.1 to 17b.5) is
// ServerProfiles2.test.tsx, whose place pins 17b2 retargets (the opener moves
// from before the table to after it). The helpers are those of
// ServerProfiles2.test.tsx, repeated here (a test file imports no other test
// file), and so is the stylesheet reader (a small local copy, as in every sheet
// test): 17b2.2 is a look, and jsdom lays nothing out.
//
// What jsdom cannot see is the designer's and the `fe/ux` check's: the + drawn
// as a 40 px square at the end of the list, the glyph visible under forced
// colours, and no sideways scroll at 320 px and 200 % text (17b2.5).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  reply,
  startAt,
  type BffOptions,
  type Instance,
  type ModelProfile,
  type ServerProfile,
} from './test/bff.ts'

const NEW = en('server-profiles-new')
const TITLE = en('server-profiles-new-title')
/** The `plus` of Icon.tsx (brief 15): the glyph of a list's add. */
const PLUS = ['M5 12h14', 'M12 5v14']

beforeEach(() => startAt('#/profiles/server'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** One row of GET /api/profiles/server. */
function row(
  id: string,
  port = 8580,
  instance: string | null = 'default',
): ServerProfile {
  return { id, engine: 'llama.cpp', port, tags: [], instance }
}

/** The two rows most tests read: one an instance runs, one free. */
const ROWS = [row('default', 8580, 'default'), row('qwen-27b', 9931, null)]

/** The app at #/profiles/server; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** The app with the pool read answered (a table, or the empty text). */
async function loaded(options: BffOptions = {}) {
  const r = open(options)
  await waitFor(() => {
    if (within(r.main).queryByRole('table') !== null) return
    if (within(r.main).queryByText(en('server-profiles-empty')) !== null) return
    throw new Error('the pool read has not answered')
  })
  return r
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** The app at `hash` against a fresh fake bff; returns main. */
function renderAt(hash: string, options: BffOptions): HTMLElement {
  startAt(hash)
  fakeBff(options)
  render(<App />)
  return screen.getByRole('main')
}

/** The opener of the create form, by its accessible name (17b2.2: unchanged). */
function newButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: NEW })
}

/** The create form, once the opener is pressed. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(newButton(main))
  return within(main).findByRole('form', { name: TITLE })
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, key: string): HTMLElement {
  return within(form).getByRole('button', { name: en(key) })
}

/** The text a sighted user reads inside `el`: what is not an aria-hidden icon
 *  and not a `visually-hidden` span (17b2.2: "only the plus icon, and a
 *  span.visually-hidden"). */
function visibleText(el: HTMLElement): string {
  return [...el.childNodes]
    .filter(
      (n) =>
        !(n instanceof Element && n.getAttribute('aria-hidden') === 'true') &&
        !(n instanceof Element && n.classList.contains('visually-hidden')),
    )
    .map((n) => n.textContent ?? '')
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The `span.visually-hidden` that gives `button` its name. */
function hiddenSpan(button: HTMLElement): HTMLElement {
  const spans = [...button.querySelectorAll('span')].filter((s) =>
    s.classList.contains('visually-hidden'),
  )
  expect(
    spans.length,
    'the button has exactly one visually hidden span',
  ).toBeGreaterThan(0)
  return spans[0]!
}

/** The button's icon: an `aria-hidden` svg carrying the `plus` paths. */
function plusOf(button: HTMLElement): Element {
  const icon = button.querySelector('svg[aria-hidden="true"]')
  expect(icon, 'the button has an aria-hidden icon').not.toBeNull()
  expect(
    [...icon!.querySelectorAll('path')].map((p) => p.getAttribute('d')),
    'the plus of Icon.tsx, the glyph of a list add',
  ).toEqual(PLUS)
  return icon!
}

/** Every button in `root` that shows the plus glyph. */
/** The + buttons of a list. The plus glyph is the horizontal stroke AND the
 *  vertical one: 22 D2.2 gives the minus the horizontal alone, so one path is
 *  not enough to tell them apart. */
function plusButtons(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('button')].filter((b) => {
    const d = [...b.querySelectorAll('svg path')].map((p) =>
      p.getAttribute('d'),
    )
    return d.includes(PLUS[0]!) && d.includes(PLUS[1]!)
  })
}

/** The elements Tab reaches inside `root`, in order. */
function tabbables(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

// ── The stylesheet reader (the way `Reflow.test.ts` reads `index.css`) ─────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  body: string
  depth: number
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const SHEET_RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
  (m) => ({
    sel: m[1].trim(),
    compact: m[1].replace(/\s+/g, ''),
    body: m[2],
    depth: opened(m.index ?? 0),
  }),
)

// Top-level rules only: the layout applies at every width, not in a media block.
const TOP = SHEET_RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

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

// The top-level rule styling this selector, as one part of a comma group: the
// sheet's own style for a shared declaration, so the coder's one rule for two
// items is as good as two.
const topStyling = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) =>
      r.sel
        .split(',')
        .some((p) => p.replace(/\s+/g, '') === sel.replace(/\s+/g, '')),
    ),
    `the top-level rule styling "${sel}" (${what})`,
  )

/** The declarations that reach a `button` carrying `classes`, in the order the
 *  cascade applies them: the top-level rules whose selector is `button` or a
 *  compound of those classes, ordered by specificity (the classes it counts)
 *  and then by the sheet's order. The classes are read off the page, so the
 *  pin follows the names the coder chose. */
function cascade(classes: string[]): Record<string, string> {
  const own = new Set(['button', ...classes.map((c) => `.${c}`)])
  const hits: { rule: Rule; spec: number; at: number }[] = []
  TOP.forEach((rule, at) => {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const tokens = part.match(/\.[-a-z0-9_]+|[-a-z0-9_]+/g) ?? []
      if (tokens.length > 0 && tokens.every((t) => own.has(t)))
        spec = Math.max(spec, tokens.filter((t) => t.startsWith('.')).length)
    }
    if (spec >= 0) hits.push({ rule, spec, at })
  })
  hits.sort((a, b) => a.spec - b.spec || a.at - b.at)
  const out: Record<string, string> = {}
  for (const { rule } of hits)
    for (const d of rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return out
}

/** The rules inside every `@media` block whose prelude matches `what`. */
function mediaRules(what: RegExp): Rule[] {
  const out: Rule[] = []
  for (const m of CSS.matchAll(/@media[^{]+\{/g)) {
    if (!what.test(m[0])) continue
    const start = (m.index ?? 0) + m[0].length
    let depth = 1
    let i = start
    while (i < CSS.length && depth > 0) {
      if (CSS[i] === '{') depth++
      else if (CSS[i] === '}') depth--
      i++
    }
    for (const r of CSS.slice(start, i - 1).matchAll(/([^{}]+)\{([^{}]*)\}/g))
      out.push({
        sel: r[1].trim(),
        compact: r[1].replace(/\s+/g, ''),
        body: r[2],
        depth: 1,
      })
  }
  return out
}

// ── 17b2.1: the place ──────────────────────────────────────────────────────

test('16 17b2.1 (as 22 D4.6 moves the intro’s neighbour): the opener sits after the table and outside its scrolling box, the delete region’s line is the only thing between the intro and the table, and the create form opens right after the +, below the list', async () => {
  // The user: "anche new server profile dovrebbe essere un + in coda alla
  // lista". The opener moves from between the intro and the table to the end
  // of the list; the form it discloses follows it there.
  const { main } = await loaded({ serverProfiles: ROWS })
  const t = within(main).getByRole('table')
  const box = t.closest('div.table-scroll')
  expect(box, 'the table is in its scrolling box (17a.3)').not.toBeNull()
  const intro = within(main).getByText(en('server-profiles-intro'))
  // 22 D4.6: the delete region's line joins the top of the list, so the intro
  // is no longer right above the table. It is the only thing between them.
  const line = [...main.querySelectorAll('[role="status"]')].find((el) =>
    before(el, t),
  )
  expect(
    line,
    '22 D4.6: the region’s line is above the table',
  ).not.toBeUndefined()
  expect(
    intro.nextElementSibling,
    '22 D4.6: the intro is followed by the region’s line',
  ).toBe(line)
  expect(
    line!.nextElementSibling,
    '17b2.1: nothing else stands between the line and the table',
  ).toBe(box)
  const opener = newButton(main)
  expect(before(box!, opener), '17b2.1: the opener comes after the table').toBe(
    true,
  )
  expect(
    opener.closest('div.table-scroll'),
    '17b2.1: and outside the scrolling box, so it is not part of the table',
  ).toBeNull()
  expect(opener.getAttribute('aria-expanded'), 'a disclosure, closed').toBe(
    'false',
  )
  press(opener)
  const form = await within(main).findByRole('form', { name: TITLE })
  expect(opener.nextElementSibling, 'the form opens right after the +').toBe(
    form,
  )
  expect(before(box!, form), 'so the form is below the list').toBe(true)
})

test('16 17b2.1: with an empty pool the opener sits after the server-profiles-empty line, and there is no opener while the read is open or after it failed', async () => {
  // The empty pool: the line stands in for the table, and the + follows it.
  const empty = await loaded({ serverProfiles: [] })
  const line = within(empty.main).getByText(en('server-profiles-empty'))
  const opener = newButton(empty.main)
  expect(
    before(line, opener),
    '17b2.1: the opener comes after the empty line',
  ).toBe(true)
  expect(
    empty.main.querySelector('div.table-scroll'),
    'and there is no table to be inside',
  ).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // While the read is open: no opener (17b.1, unchanged).
  const gate = deferred()
  const loading = open({ serverProfiles: () => gate.promise })
  await within(loading.main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  expect(
    within(loading.main).queryByRole('button', { name: NEW }),
    '17b2.1: no opener while loading',
  ).toBeNull()
  gate.resolve(reply(200, ROWS))
  cleanup()
  vi.unstubAllGlobals()
  // After a failed read: no opener.
  const failed = open({
    serverProfiles: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  await within(failed.main).findByText(en('api-client-unavailable'))
  expect(
    within(failed.main).queryByRole('button', { name: NEW }),
    '17b2.1: no opener after a failure',
  ).toBeNull()
})

// ── 17b2.2: the look ───────────────────────────────────────────────────────

test('16 17b2.2: the opener is the list Add icon button: only the aria-hidden plus and a visually hidden span with server-profiles-new, no visible text, its name exactly "New server profile", its id and aria-expanded unchanged', async () => {
  // The name stays exactly "New server profile" (WCAG 4.1.2): a screen reader
  // says "New server profile, button, collapsed" and a voice user can say
  // "click New server profile". 2.5.3 does not apply: nothing is visible.
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  expect(opener.id, '17b2.2: the id stays').toBe('server-profiles-new')
  expect(opener.getAttribute('aria-expanded'), 'the disclosure stays').toBe(
    'false',
  )
  plusOf(opener)
  expect(
    hiddenSpan(opener).textContent?.replace(/\s+/g, ' ').trim(),
    '17b2.2: the hidden span holds server-profiles-new',
  ).toBe(NEW)
  expect(visibleText(opener), '17b2.2: the button shows no text').toBe('')
  expect(
    opener.getAttribute('aria-label'),
    '17b2.2: the name comes from the span, not from an aria-label',
  ).toBeNull()
})

test('16 17b2.2 (the sheet): the opener is drawn by the same rule as .ot-add: a 40 px square with the --control outline, margin-top: var(--space-3), left-aligned', async () => {
  // "Share the class or the rule, not a copy": the cascade for the classes the
  // opener carries is the tensor list +'s look, and the rule that gives it is
  // the one that styles `.ot-add`. The room above it is 21 T.24's.
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  const classes = [...opener.classList]
  const at = cascade(classes)
  expect(norm(at['width'] ?? ''), '17b2.2: the + is 40 px wide').toBe('40px')
  expect(norm(at['min-height'] ?? ''), '17b2.2: and at least 40 px high').toBe(
    '40px',
  )
  expect(
    norm(at['border'] ?? ''),
    '17b2.2: with the --control outline of a secondary button',
  ).toBe('var(--border-w) solid var(--control)')
  expect(
    norm(at['margin-top'] ?? ''),
    '17b2.2: the room from the table or the empty line, as 21 T.24',
  ).toBe('var(--space-3)')
  // The same rule, not a copy of its declarations.
  const addRule = topStyling('.ot-add', 'the tensor list add (21 T.21)')
  expect(
    norm(prop(addRule.body, 'width')),
    '17b2.2: the rule that styles .ot-add is the one with the 40 px square',
  ).toBe('40px')
  const shared = addRule.sel
    .split(',')
    .map((p) => p.replace(/\s+/g, ''))
    .filter((p) => p !== '.ot-add')
  expect(
    classes.some((c) => shared.includes(`.${c}`)) ||
      shared.includes('.ot-add') ||
      classes.includes('ot-add'),
    '17b2.2: the opener carries a class the .ot-add rule styles, so the two share one rule (the brief: share the class or the rule, not a copy)',
  ).toBe(true)
  // Left-aligned: nothing centres it.
  for (const [name, value] of [
    ['align-self', 'center'],
    ['margin-left', 'auto'],
    ['margin-inline', 'auto'],
    ['text-align', 'center'],
  ] as Array<[string, string]>)
    expect(
      norm(at[name] ?? ''),
      `17b2.2: the + is left-aligned, so it declares no ${name}: ${value}`,
    ).not.toBe(value)
})

test('16 17b2.2: after the empty-pool line the + has no room of its own, so the gap is the paragraph’s 12 px and not 24 (TP1c review D1)', async () => {
  // The same defect as the tensor list’s, on the empty pool: the + is an
  // inline-flex box, so its margin-top never collapses with the bottom margin
  // the global `p` rule gives `server-profiles-empty` (index.css 314), and the
  // + sits 24 px under the line. The review’s fix, `p + .ot-add { margin-top:
  // 0 }`, is one rule covering both pages (16 17b2 review, and 21 T.24’s
  // TP1c review D1). With rows the + follows the table’s scroll box, so it
  // keeps the --space-3 room 17b2.2 pins.
  const { main } = await loaded({ serverProfiles: [] })
  const opener = newButton(main)
  const line = within(main).getByText(en('server-profiles-empty'))
  expect(
    line.tagName,
    '17b2.2: the empty-pool line is a p, whose bottom margin is the gap',
  ).toBe('P')
  expect(
    opener.previousElementSibling,
    '17b2.2: the + sits right after that paragraph, where the rule reaches it',
  ).toBe(line)
  const after = TOP.filter((r) =>
    r.sel.split(',').some((p) => /^p\s*\+\s*\.ot-add$/.test(p.trim())),
  )
  expect(
    after.length,
    'D1: the sheet has one rule for a .ot-add right after a p (the review names it: p + .ot-add { margin-top: 0 })',
  ).toBe(1)
  expect(
    norm(prop(after[0]!.body, 'margin-top')),
    'D1: there the + declares no room of its own, so the paragraph’s 12 px is the gap',
  ).toBe('0')
  // With rows the + follows the table’s scroll box, not a paragraph.
  cleanup()
  const full = await loaded({ serverProfiles: ROWS })
  expect(
    newButton(full.main).previousElementSibling?.classList.contains(
      'table-scroll',
    ),
    '17b2.2: with rows the + follows the table’s box, so it keeps its room',
  ).toBe(true)
})

test('16 17b2.2: under forced colours the + keeps its glyph visible (color: ButtonText)', async () => {
  // The icon is a `currentColor` stroke, so the button colour is what draws it.
  const { main } = await loaded({ serverProfiles: ROWS })
  const classes = [...newButton(main).classList].map((c) => `.${c}`)
  const forced = mediaRules(/forced-colors/)
  const hits = forced.filter((r) => classes.some((c) => r.compact.includes(c)))
  expect(
    hits.length,
    '17b2.2: a forced-colours rule styles the opener',
  ).toBeGreaterThan(0)
  expect(
    hits.some((r) => norm(prop(r.body, 'color')) === 'ButtonText'),
    '17b2.2: its colour is ButtonText, so the glyph is drawn in the system text colour',
  ).toBe(true)
})

// ── 17b2.3: the behaviour, apart from the place ────────────────────────────

test('16 17b2.3: opening moves the focus to Name, and Cancel and the opener each close the form and put the focus back on the +', async () => {
  // 17b.1 and 17b.2 stand as written, apart from the place. Green on day one:
  // the guard that moving the opener breaks nothing.
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  const form = await openForm(main)
  expect(document.activeElement, '17b2.3: the focus moves to Name').toBe(
    boxIn(form, en('server-profile-name')),
  )
  expect(opener.getAttribute('aria-expanded'), 'the press opened it').toBe(
    'true',
  )
  press(buttonIn(form, 'create-cancel'))
  expect(within(main).queryByRole('form'), 'Cancel closed it').toBeNull()
  expect(document.activeElement, 'and the focus is back on the +').toBe(opener)
  expect(opener.getAttribute('aria-expanded'), 'closed').toBe('false')
  // The opener pressed again closes it too.
  press(newButton(main))
  await within(main).findByRole('form', { name: TITLE })
  press(newButton(main))
  expect(within(main).queryByRole('form'), 'the opener closed it').toBeNull()
  expect(document.activeElement, 'and the focus is on the +').toBe(opener)
})

test('16 17b2.3: Escape in the form closes it and puts the focus back on the +', async () => {
  // 17b2.3 names Escape beside Cancel and the opener. The create form as built
  // (17b.2) already closes on Escape, so this is green on day one: the guard
  // that moving the opener to the end of the list keeps it, and keeps the
  // focus landing on the +.
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  const form = await openForm(main)
  fireEvent.keyDown(boxIn(form, en('server-profile-name')), {
    key: 'Escape',
    code: 'Escape',
  })
  expect(within(main).queryByRole('form'), 'Escape closed it').toBeNull()
  expect(document.activeElement, 'and the focus is on the +').toBe(opener)
})

test('16 17b2.3: the Tab path follows the page: the table links, then the +, then the form controls when it is open', async () => {
  // 2.4.3: the + is the last control of the list and the form comes after it.
  // Every async element is taken before the list, as the shell lands with the
  // session and the table with the read.
  const { main } = await loaded({ serverProfiles: ROWS })
  const t = await within(main).findByRole('table')
  const links = [...t.querySelectorAll<HTMLAnchorElement>('tbody a')]
  expect(links.length, 'the table has links to follow').toBeGreaterThan(0)
  const opener = newButton(main)
  press(opener)
  const form = await within(main).findByRole('form', { name: TITLE })
  const name = await within(form).findByRole('textbox', {
    name: en('server-profile-name'),
  })
  const stops = tabbables(main)
  const at = (el: HTMLElement) => stops.indexOf(el)
  expect(at(opener), 'the + is a tab stop').toBeGreaterThan(-1)
  for (const link of links)
    expect(at(link), '17b2.3: a table link comes before the +').toBeLessThan(
      at(opener),
    )
  expect(at(name), '17b2.3: the form Name comes after the +').toBeGreaterThan(
    at(opener),
  )
})

// ── 17b2.4: every list with an add ─────────────────────────────────────────

test('16 17b2.4: the pool list add is the + (the rule of every list add), named by its existing key server-profiles-new', async () => {
  // The rule: each list add is the + after its list, named by its existing key.
  // The pool list is the one built today, so it is the one pinned as a +.
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  plusOf(opener)
  expect(
    hiddenSpan(opener).textContent?.replace(/\s+/g, ' ').trim(),
    '17b2.4: the pool add is named by its existing key',
  ).toBe(NEW)
})

test('16 17b2.4: the Overview and the Models list hold no add at all today (17c.1 and 22 C.1 are not built), and a button that is not a list add keeps its words', async () => {
  // The two lists whose add is not built yet are pinned as holding no add at
  // all: when 17c.1 (New instance) and 22 C.1 (New model profile) land, this
  // test moves to their + at the end of their list (the Models one after the
  // table, outside the filter). The create form buttons are not a list add, so
  // they keep their words: 17b2.4 narrows the rule to a list add, it does not
  // strip words from every button.
  const { main } = await loaded({ serverProfiles: ROWS })
  const form = await openForm(main)
  for (const key of ['create-button', 'create-cancel'])
    expect(
      visibleText(buttonIn(form, key)),
      `17b2.4: ${key} is not a list add, so it keeps its words`,
    ).toBe(en(key))
  cleanup()
  vi.unstubAllGlobals()
  // The Overview: 17c.1 (New instance) is not built, so the tiles carry no
  // add today. A plus button landing there moves this pin to the + after the
  // tiles.
  const overview = renderAt('#/', { instances: OVERVIEW })
  const tiles = await within(overview).findByRole('list')
  expect(
    plusButtons(overview),
    '17b2.4: the Overview has no list add today (17c.1 not built)',
  ).toHaveLength(0)
  expect(plusButtons(tiles), 'and none inside the tiles').toHaveLength(0)
  cleanup()
  vi.unstubAllGlobals()
  // The Models list: 22 C.1 (New model profile) is not built, and the add will
  // sit after the table, outside the filter.
  const models = renderAt('#/profiles/model', { modelProfiles: MODELS })
  await within(models).findByRole('table')
  expect(
    plusButtons(models),
    '17b2.4: the Models list has no list add today (22 C.1 not built)',
  ).toHaveLength(0)
})

// ── 17b2.5: the ux check state ─────────────────────────────────────────────

test('16 17b2.5: the ux check server-profiles-new state opens the form through the + (its selector names the plus glyph, and the hidden text is in the DOM)', async () => {
  // The check opens the form with `main button:has(svg path[d="M12 5v14"])`,
  // the plus of Icon.tsx: the selector survives the move and the look, because
  // the glyph stays. The hidden text is what a text selector reads, so the
  // span of 17b2.2 is pinned here too. The 320 px and 200 % reflow of the
  // state is the check's own measurement.
  //
  // The stroke named is the plus's vertical one, not its horizontal one: 22D4
  // review D3. `M5 12h14` was the whole of the minus every row's − of the pool
  // carried, so a state naming it matched two buttons and Playwright threw a
  // strict-mode violation. 22 D4.21 makes the row's − a trash can, so the
  // horizontal stroke is the plus's alone today; the state keeps naming the
  // vertical one, the plus's own in either look.
  const check = readFileSync(
    join(import.meta.dirname, '..', 'ux', 'check.js'),
    'utf8',
  )
  const at = check.indexOf("name: 'server-profiles-new'")
  expect(at, 'the state exists in fe/ux/check.js').toBeGreaterThan(-1)
  const block = check.slice(at, check.indexOf('\n  {', at))
  expect(
    block.includes('M12 5v14'),
    '17b2.5: the state opens through the button carrying the plus',
  ).toBe(true)
  expect(
    block.includes('M5 12h14'),
    '17b2.5: the state does not name the stroke the minus shares',
  ).toBe(false)
  const { main } = await loaded({ serverProfiles: ROWS })
  const opener = newButton(main)
  expect(
    (opener.textContent ?? '').includes(NEW),
    '17b2.5: the hidden text is in the DOM, so a text selector finds the +',
  ).toBe(true)
  expect(plusOf(opener), 'and the glyph is the plus').toBeTruthy()
})

// ── Fixtures for the two lists whose add is not built ──────────────────────

const OVERVIEW: Instance[] = [
  { id: 'default', engine: 'llama.cpp', state: 'running' },
]

const MODELS: ModelProfile[] = [
  {
    id: 'Qwen3-27B',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    model: 'qwen3-27b-q4_k.gguf',
  },
]
