// Brief 22, slice 22D2 (fe/briefs/22-model-sources.md, section 22D2): the delete
// of a model profile becomes a minus icon button in a new status row under the
// summary line, the server profile page follows with the same row and its
// "used by" status, and the Save bar moves to the top of both pages, sticky at
// the viewport's top. The answers of 22 D.1 to 22 D.7 (ModelProfileDelete.test.tsx)
// and of 16 17b.6 to 17b.9 (ServerProfilePage2.test.tsx) stand: what this slice
// changes there is the place of the opener, of the muted line, of the button
// after the Value list and of the bar in the sheet, as 22 D2.10 lists.
//
// 22 D2.9's widths (320 px, 200 % text, 1280 and 1600 px, both themes, no
// sideways scroll, the minus wrapping under the status text, the bar never
// covering a focused control) are measurements: the UX check's states
// `model-profile-delete` and `server-profile-delete` measure them, and this
// slice retargets those two states to the two ids (fe/ux/check.js). jsdom lays
// nothing out, so what the row and the bar must declare to wrap and to clear a
// focused control is pinned here as the sheet text, the way 2h.135, H5c.1 and
// the 22D review D2 test do. The reader is a small local copy: a test file
// imports no other test file.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  en,
  EN,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  serverProfileDoc,
  shownText,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const ROUTE = `/api/profiles/model/${encodeURIComponent(ID)}`
const SERVER_ID = 'qwen-27b'
const SERVER_HASH = `#/profiles/server/${SERVER_ID}`
const SERVER_ROUTE = `/api/profiles/server/${SERVER_ID}`
/** The instance that takes the server profile in 22 D2.8's 409. */
const TAKEN = 'coder'
/** The minus of `Icon.tsx`: the horizontal stroke of the `plus` family, with
 *  no vertical stroke. Lucide's icons/minus.svg, the same 24 unit stroke family
 *  the brief asks for (22 D2.2). */
const MINUS = 'M5 12h14'
const PLUS_VERTICAL = 'M12 5v14'
/** The muted line 22 D.1 put beside the bottom Delete button, whose key 22 D2.10
 *  takes out of the strings file. Written out, because `en()` reads the map the
 *  key leaves: the pin is that these words are on no page any more. */
const NEEDS_OFF =
  'To delete this model profile, switch it off first in the list of model profiles.'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The document of a switched-off model profile. */
function offDoc() {
  return reply(200, modelProfileDoc({}, { enabled: false }))
}

/** The document of a switched-on model profile. */
function onDoc() {
  return reply(200, modelProfileDoc({}, { enabled: true }))
}

/** The model profile page, its document read as switched off. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfile: () => offDoc(), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

function settingsTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
}

/** The status row of 22 D2.1. */
function statusRow(main: HTMLElement): Element | null {
  return main.querySelector('div.page-status')
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('model-profile-delete') })
}

function saveBar(main: HTMLElement): HTMLElement | null {
  return main.querySelector('.save-bar')
}

/** The confirm group's accessible name: its question (22 D.2). */
const QUESTION = en('model-profile-delete-question', ID)

function queryConfirm(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('group', { name: QUESTION })
}

/** The confirm group, once the opener is pressed. */
async function confirmGroup(main: HTMLElement): Promise<HTMLElement> {
  press(deleteButton(main))
  return within(main).findByRole('group', { name: QUESTION })
}

function confirmIn(group: HTMLElement, key: string): HTMLElement {
  return within(group).getByRole('button', { name: en(key) })
}

/** The elements Tab reaches inside `main`, named as Tab names them (2e.91). */
function mainTabbable(main: HTMLElement): string[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => nameOf(el))
}

/** The name Tab shows: a control's text, or the label that names an input. */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  return ''
}

// The server profile page (22 D2.8).

/** The document of a server profile no instance runs. */
function freeDoc() {
  return reply(200, serverProfileDoc({}, { instance: null }))
}

/** The document of a server profile instance `instance` runs. */
function usedDoc(instance: string) {
  return reply(200, serverProfileDoc({}, { instance }))
}

/** The server profile page, its document read as a free profile. */
async function loadedServer(options: BffOptions = {}) {
  startAt(SERVER_HASH)
  const bff = fakeBff({ serverProfile: () => freeDoc(), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

function serverDeleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', {
    name: en('server-profile-delete'),
  })
}

const SERVER_QUESTION = en('server-profile-delete-question', SERVER_ID)

// ── The stylesheet reader (22 D2.2, D2.6, D2.9) ─────────────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = { compact: string; body: string; at: number; depth: number }

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the bar is sticky at every width but in the short
// viewport block, which is read on its own.
const TOP = RULES.filter((r) => r.depth === 0)

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length ? ` (${list.map((r) => r.compact).join(' | ')})` : ''),
    )
  return list[0]
}

const topBy = (compact: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === compact),
    `the top-level rule whose selector is exactly ${compact} (${what})`,
  )

// The rules inside the @media (max-height: 31.98rem) block, by its own matched
// braces.
const shortBlock = (): Rule[] => {
  const open = CSS.indexOf('@media (max-height: 31.98rem)')
  expect(open, 'the short-viewport media block').toBeGreaterThan(-1)
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) break
  }
  return RULES.filter((r) => r.depth > 0 && r.at > open && r.at < end)
}

/** The declarations that reach `el` from the top-level rules, merged in the
 *  cascade's order, with the rules that carry them. The reader of the 22D
 *  review D2 test (ServerProfilePage2.test.tsx), a local copy, keeping the
 *  rules so a pin can name the rule a value comes from. A selector with a
 *  descendant, a pseudo-class, an attribute or a combinator is skipped: it
 *  needs a tree jsdom does not lay out. The tag, the classes and the id are
 *  read off the page, so the pin follows the names the coder chose. */
function cascade(el: HTMLElement): {
  decls: Record<string, string>
  rules: Rule[]
} {
  const own = new Set([el.tagName.toLowerCase()])
  for (const c of el.classList) own.add(`.${c}`)
  if (el.id) own.add(`#${el.id}`)
  const hits: { rule: Rule; spec: number }[] = []
  TOP.forEach((rule) => {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const tokens = part.match(/[#.]?[-a-z0-9_]+/g) ?? []
      if (tokens.length > 0 && tokens.every((t) => own.has(t)))
        spec = Math.max(spec, tokens.filter((t) => t.startsWith('.')).length)
    }
    if (spec >= 0) hits.push({ rule, spec })
  })
  hits.sort((a, b) => a.spec - b.spec || a.rule.at - b.rule.at)
  const decls: Record<string, string> = {}
  for (const h of hits)
    for (const d of h.rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) decls[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return { decls, rules: hits.map((h) => h.rule) }
}

// ── 22 D2.1: the status row ─────────────────────────────────────────────

test('22 D2.1: right after the summary line a div.page-status holds the status paragraph, and the minus only while the read is switched off; switched on the row says model-profile-on, and the bottom button, the muted line and the off line before the section are gone', async () => {
  const { main } = await loaded()
  const summary = main.querySelector('p.model-profile-summary')
  expect(summary, 'the summary line is there').not.toBeNull()
  const row = statusRow(main)
  expect(row, 'the status row').not.toBeNull()
  expect(
    summary!.nextElementSibling,
    '22 D2.1: the row is right after the summary line',
  ).toBe(row)
  const h2 = settingsTitle(main)
  expect(before(row!, h2), 'and sits above the settings section').toBe(true)
  expect(row!.closest('section'), 'above the section, so full width').toBeNull()
  const para = row!.querySelector('p')
  expect(para, 'the row holds a paragraph').not.toBeNull()
  expect(
    text(para),
    'a switched-off profile says it is off, the words of 22D',
  ).toBe(en('model-profile-off'))
  const button = deleteButton(main)
  expect(button.parentElement, 'the minus is in the row').toBe(row)
  expect(button, 'and last in it, at the paragraph’s right').toBe(
    row!.lastElementChild,
  )
  const bar = saveBar(main)
  expect(bar, 'the save bar is there').not.toBeNull()
  expect(before(row!, bar!), 'the row is above the bar').toBe(true)
  expect(
    within(main).queryByText(NEEDS_OFF),
    'a switched-off profile says nothing about switching it off',
  ).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // A switched-on profile: no minus, and the row says what the state is and
  // where to change it (the words replace the muted line of 22 D.1).
  const on = await loaded({ modelProfile: () => onDoc() })
  const onRow = statusRow(on.main)
  expect(onRow, 'the row is there for a switched-on profile').not.toBeNull()
  expect(
    text(onRow!.querySelector('p')),
    '22 D2.1: the row says model-profile-on',
  ).toBe(en('model-profile-on'))
  expect(
    within(on.main).queryByRole('button', {
      name: en('model-profile-delete'),
    }),
    'no minus for a switched-on profile',
  ).toBeNull()
  expect(
    within(on.main).queryByText(NEEDS_OFF),
    '22 D2.1: the muted line is gone, its key leaves the strings file',
  ).toBeNull()
  expect(
    onRow!.nextElementSibling,
    'nothing of the delete is left below the row',
  ).toBe(on.main.querySelector('section'))
})

// ── 22 D2.2: the minus button ───────────────────────────────────────────

test('22 D2.2: the minus is a type="button" with id model-profile-delete and aria-expanded, holding a minus icon aria-hidden and a span.visually-hidden that names it exactly, in the 40 px outlined square the list add shares (.ot-add) with flex: none', async () => {
  const { main } = await loaded()
  const button = deleteButton(main)
  expect(button.tagName, 'a native button').toBe('BUTTON')
  expect(button.getAttribute('type'), 'it submits nothing').toBe('button')
  expect(button.id, '22 D2.2: the id the UX check finds it by').toBe(
    'model-profile-delete',
  )
  expect(button.getAttribute('aria-expanded'), 'closed').toBe('false')
  const svg = button.querySelector('svg')
  expect(svg, 'the icon').not.toBeNull()
  expect(svg!.getAttribute('aria-hidden'), 'decorative').toBe('true')
  const paths = [...svg!.querySelectorAll('path')].map((p) =>
    p.getAttribute('d'),
  )
  expect(
    paths,
    '22 D2.2: the minus of the plus stroke family: the horizontal line, and no vertical one',
  ).toEqual([MINUS])
  expect(
    paths,
    'and not the plus of the list add: no vertical stroke',
  ).not.toContain(PLUS_VERTICAL)
  const hidden = button.querySelector('span.visually-hidden')
  expect(hidden, 'the words, for a reader').not.toBeNull()
  expect(
    text(hidden),
    '4.1.2: the accessible name stays exactly the words of 22D',
  ).toBe(en('model-profile-delete'))
  expect(shownText(button), '2.5.3: nothing is written on the button').toBe('')
  const { decls, rules } = cascade(button)
  expect(norm(decls['width'] ?? ''), 'the 40 px square').toBe('40px')
  expect(norm(decls['min-height'] ?? ''), 'and never shorter').toBe('40px')
  expect(norm(decls['padding'] ?? ''), 'no padding').toBe('0')
  expect(
    norm(decls['border'] ?? ''),
    'the --control outline of a control',
  ).toBe('var(--border-w) solid var(--control)')
  expect(
    norm(decls['flex'] ?? ''),
    '22 D2.9: kept at its box, so it wraps under the text and never stretches',
  ).toBe('none')
  const square = rules.filter((r) => prop(r.body, 'width') === '40px')
  expect(square.length, 'one rule gives the box its width').toBe(1)
  expect(
    square[0]!.compact.split(','),
    '22 D2.2: it is the rule of the list add, shared and not copied',
  ).toContain('.ot-add')
})

// ── 22 D2.3: the confirm opens under the row ────────────────────────────

test('22 D2.3: pressing the minus opens the confirm group right after the status row, full width, with margin-top var(--space-3); the focus moves to Cancel, Cancel and Escape put it back on the minus, and nothing is sent', async () => {
  const { bff, main } = await loaded()
  const row = statusRow(main)
  expect(row, 'the status row').not.toBeNull()
  const opener = deleteButton(main)
  const group = await confirmGroup(main)
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  expect(
    row!.nextElementSibling,
    '22 D2.3: the group comes right after the row, not after the opener',
  ).toBe(group)
  expect(group.parentElement, 'the group is the row’s sibling').toBe(
    row!.parentElement,
  )
  expect(group.closest('section'), 'full width, outside the section').toBeNull()
  expect(
    norm(cascade(group).decls['margin-top'] ?? ''),
    '22 D2.3: the space under the row, the same as the server page’s',
  ).toBe('var(--space-3)')
  expect(
    opener.getAttribute('aria-expanded'),
    'the opener says it is open',
  ).toBe('true')
  const cancel = confirmIn(group, 'confirm-cancel')
  expect(document.activeElement, '22 D.2: the focus is on Cancel').toBe(cancel)
  expect(
    bff.to('DELETE', ROUTE),
    'nothing is sent before the verb',
  ).toHaveLength(0)
  press(cancel)
  expect(queryConfirm(main), 'Cancel closes it').toBeNull()
  expect(document.activeElement, 'and the focus is back on the minus').toBe(
    opener,
  )
  const again = await confirmGroup(main)
  expect(document.activeElement, 'Cancel again').toBe(
    confirmIn(again, 'confirm-cancel'),
  )
  fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' })
  expect(queryConfirm(main), 'Escape closes it').toBeNull()
  expect(document.activeElement, 'on the minus').toBe(opener)
  expect(bff.to('DELETE', ROUTE), 'still nothing sent').toHaveLength(0)
})

// ── 22 D2.4: switched on meanwhile ──────────────────────────────────────

test('22 D2.4: after the 409 model-profile-enabled and the read again the row says model-profile-on and the minus is gone, and p#model-profile-delete-refused[role="alert"][tabindex="-1"] sits right after the row with the focus', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    modelProfile: () => (++reads === 1 ? offDoc() : onDoc()),
    modelProfileDelete: () =>
      reply(409, { key: 'model-profile-enabled', args: [ID] }),
  })
  const refusal = en('api-model-profile-enabled', ID)
  const row = statusRow(main)
  expect(row, 'the status row').not.toBeNull()
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  await within(group).findByText(refusal)
  // The read again lands: the row says the profile is on, the minus and its
  // group go with it, and the alert must outlive them.
  await within(main).findByText(en('model-profile-on'))
  expect(
    within(main).queryByRole('button', { name: en('model-profile-delete') }),
    'the minus is gone',
  ).toBeNull()
  expect(queryConfirm(main), 'and so is the confirm').toBeNull()
  expect(readCount(bff), 'the document was read again').toBe(2)
  const kept = within(main).getByText(refusal)
  expect(kept.tagName, 'the text is in a paragraph').toBe('P')
  expect(kept.id, '22 D2.4: the id 22D gives it').toBe(
    'model-profile-delete-refused',
  )
  expect(kept.getAttribute('role'), 'an alert').toBe('alert')
  expect(kept.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(
    row!.nextElementSibling,
    '22 D2.4: it comes right after the row, where the group was',
  ).toBe(kept)
  expect(document.activeElement, 'and it has the focus').toBe(kept)
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
})

/** The document reads the page sent. */
function readCount(bff: ReturnType<typeof fakeBff>, id = ID): number {
  return bff.to('GET', `/api/profiles/model/${encodeURIComponent(id)}`).length
}

// ── 22 D2.5: the Save bar at the top ────────────────────────────────────

test('22 D2.5: the Save bar is inside the settings section, right after its h2, before the jump list and the cards, with its status line, Discard and Save changes', async () => {
  const { main } = await loaded()
  const h2 = settingsTitle(main)
  const section = h2.closest('section')
  expect(section, 'the h2 sits in a section').not.toBeNull()
  const bar = saveBar(main)
  expect(bar, 'the bar is there').not.toBeNull()
  expect(bar!.closest('section'), 'inside the settings section').toBe(section)
  expect(
    h2.nextElementSibling,
    '22 D2.5: right after the h2, the first thing in the section',
  ).toBe(bar)
  const nav = within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
  expect(before(bar!, nav), 'before the jump list').toBe(true)
  const assets = within(main).getByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  expect(before(bar!, assets), 'and before the cards').toBe(true)
  const statuses = [...bar!.querySelectorAll('[role="status"]')]
  expect(statuses, 'the bar keeps its one status line').toHaveLength(1)
  expect(text(statuses[0]), 'empty on arrival').toBe('')
  expect(
    within(bar!).getByRole('button', { name: en('save-discard') }).tagName,
    'Discard is in the bar',
  ).toBe('BUTTON')
  expect(
    within(bar!).getByRole('button', { name: en('save-button') }).id,
    'and Save changes carries the page’s save id',
  ).toBe('model-profile-save')
})

// ── 22 D2.6: sticky at the top ──────────────────────────────────────────

test('22 D2.6: the sheet sticks the bar to the top: .save-bar is sticky at top 0 with no bottom, its border and its round corners on the bottom edge, its shadow, background and wrap kept; html:has(.save-bar) pads scroll-padding-top and .save-bar button cancels it with the equal negative scroll-margin-top; below 32rem the bar is static and both are 0', () => {
  const bar = topBy('.save-bar', 'the bar rule')
  expect(prop(bar.body, 'position'), '22 D2.6: sticky').toBe('sticky')
  expect(prop(bar.body, 'top'), 'at the viewport’s top').toBe('0')
  expect(
    prop(bar.body, 'bottom'),
    'the bottom edge is gone with the move',
  ).toBe('')
  expect(
    norm(prop(bar.body, 'border-bottom')),
    'the border moves to the bottom edge',
  ).toBe('1px solid var(--control)')
  expect(prop(bar.body, 'border-top'), 'and not the top').toBe('')
  expect(
    norm(prop(bar.body, 'border-radius')),
    'only its bottom corners round',
  ).toBe('0 0 var(--radius-card) var(--radius-card)')
  expect(prop(bar.body, 'box-shadow'), 'the same lift').toBe('var(--shadow)')
  expect(prop(bar.body, 'background'), 'the same background').toBe('var(--bg)')
  expect(prop(bar.body, 'flex-wrap'), 'the same wrap').toBe('wrap')
  const pad = topBy('html:has(.save-bar)', 'the page padding rule')
  expect(
    norm(prop(pad.body, 'scroll-padding-top')),
    '22 D2.6: the padding follows the bar to the top, the same calc',
  ).toBe('calc(max(var(--save-bar-height, 7rem), 7rem) + var(--space-3))')
  expect(prop(pad.body, 'scroll-padding-bottom'), 'and leaves the bottom').toBe(
    '',
  )
  const btn = topBy('.save-barbutton', 'the bar’s controls rule')
  const inner = prop(pad.body, 'scroll-padding-top')
    .replace(/^calc\((.*)\)$/, '$1')
    .trim()
  expect(
    norm(prop(btn.body, 'scroll-margin-top')),
    '22 D2.6: the padding of the rule above, negated, so focusing a bar button scrolls nothing (2.4.11)',
  ).toBe(`calc(-1 * (${inner}))`)
  expect(
    prop(btn.body, 'scroll-margin-bottom'),
    'and the bottom margin is gone',
  ).toBe('')
  expect(btn.at > pad.at, 'written after the padding rule, as H5c.1 pins').toBe(
    true,
  )
  const short = shortBlock()
  expect(
    prop(
      single(
        short.filter((r) => r.compact === '.save-bar'),
        'the .save-bar rule inside the short-viewport block',
      ).body,
      'position',
    ),
    '22 D2.6: below a 32rem viewport the bar is static, at the top of the settings',
  ).toBe('static')
  expect(
    prop(
      single(
        short.filter((r) => r.compact === 'html:has(.save-bar)'),
        'the html:has(.save-bar) rule inside the short-viewport block',
      ).body,
      'scroll-padding-top',
    ),
    'and the padding is 0 there',
  ).toBe('0')
  expect(
    prop(
      single(
        short.filter((r) => r.compact === '.save-barbutton'),
        'the .save-bar button rule inside the short-viewport block',
      ).body,
      'scroll-margin-top',
    ),
    'and the cancel is cancelled there too (H5c.2)',
  ).toBe('0')
  const bottom = RULES.filter(
    (r) =>
      prop(r.body, 'scroll-padding-bottom') !== '' ||
      prop(r.body, 'scroll-margin-bottom') !== '',
  )
  expect(
    bottom.map((r) => r.compact),
    '22 D2.6: no rule in the sheet pads or margins the scroll at the bottom any more',
  ).toEqual([])
})

// ── 22 D2.7: the focus order ────────────────────────────────────────────

test('22 D2.7: Tab reaches the back link, the minus, Discard and Save changes, the jump list and then the fields; with the confirm open its two buttons come between the minus and the bar, and the bar’s buttons come before every field of the cards', async () => {
  const { main } = await loaded()
  const stops = mainTabbable(main)
  expect(
    stops.slice(0, 4),
    '22 D2.7: back link, the status row’s minus, Discard, Save changes',
  ).toEqual([
    en('model-profile-back'),
    en('model-profile-delete'),
    en('save-discard'),
    en('save-button'),
  ])
  const jump = stops.indexOf(en('model-profile-jump-label'))
  expect(jump, 'the jump list follows the bar').toBeGreaterThan(
    stops.indexOf(en('save-button')),
  )
  const assets = stops.indexOf(en('model-profile-assets'))
  expect(assets, 'and the Assets toggle follows the jump list').toBeGreaterThan(
    jump,
  )
  const firstField = stops.findIndex(
    (s) => s !== '' && !stops.slice(0, assets).includes(s),
  )
  expect(
    firstField,
    'the first field of the cards is a stop, after the bar',
  ).toBeGreaterThan(stops.indexOf(en('save-button')))
  cleanup()
  vi.unstubAllGlobals()
  // The confirm, open, is the pair right after the minus (22 D2.7).
  const off = await loaded()
  const group = await confirmGroup(off.main)
  const open = mainTabbable(off.main)
  expect(
    open.slice(1, 4),
    'the minus, then the group’s verb and Cancel, then the bar',
  ).toEqual([
    en('model-profile-delete'),
    en('confirm-delete'),
    en('confirm-cancel'),
  ])
  expect(
    open.indexOf(en('save-discard')),
    'the bar comes after the open group',
  ).toBeGreaterThan(open.indexOf(en('confirm-cancel')))
  expect(
    group.contains(document.activeElement as Node),
    'the focus is inside the group, on Cancel',
  ).toBe(true)
})

// ── 22 D2.8: the server profile page follows ────────────────────────────

test('22 D2.8: under the h1 a div.page-status holds the status paragraph and, while no instance runs the profile, the minus named by a visually hidden server-profile-delete in the .ot-add square; an instance named says server-profile-used-by and offers nothing, and the Value list’s Used by row stays', async () => {
  const { main } = await loadedServer()
  const h1 = within(main).getByRole('heading', { level: 1, name: SERVER_ID })
  const row = statusRow(main)
  expect(row, '22 D2.8: the row is under the h1').not.toBeNull()
  expect(before(h1, row!), 'right after it').toBe(true)
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list is there').not.toBeNull()
  expect(before(row!, dl!), 'and the row is above the Value list').toBe(true)
  expect(
    text(row!.querySelector('p')),
    'unused: the row says no instance uses it',
  ).toBe(en('server-profile-unused'))
  const button = serverDeleteButton(main)
  expect(button.id, 'the id the UX check finds it by').toBe(
    'server-profile-delete',
  )
  expect(button.getAttribute('type'), 'it submits nothing').toBe('button')
  expect(
    text(button.querySelector('span.visually-hidden')),
    '4.1.2: named by the words, not by the glyph',
  ).toBe(en('server-profile-delete'))
  expect(
    [...button.querySelectorAll('svg path')].map((p) => p.getAttribute('d')),
    'the same minus as the model page',
  ).toEqual([MINUS])
  expect(
    norm(cascade(button).decls['width'] ?? ''),
    'the same 40 px square',
  ).toBe('40px')
  expect(button.parentElement, 'the minus is in the row').toBe(row)
  expect(
    text(dl!.querySelector('dt')),
    '22 D2.8: the Value list keeps its Used by row, which is data',
  ).toBe(en('server-profiles-column-used-by'))
  cleanup()
  vi.unstubAllGlobals()
  // A profile an instance runs: the row names the instance, and nothing is
  // offered.
  const used = await loadedServer({ serverProfile: () => usedDoc(TAKEN) })
  const usedRow = statusRow(used.main)
  expect(
    text(usedRow!.querySelector('p')),
    '22 D2.8: the row names the instance and where to move it',
  ).toBe(en('server-profile-used-by', TAKEN))
  expect(
    within(used.main).queryByRole('button', {
      name: en('server-profile-delete'),
    }),
    'no minus for a profile in use',
  ).toBeNull()
})

test('22 D2.8: on the server page the confirm opens right after the status row, and after the 409 client-server-profile-in-use and the read again its paragraph sits right after the row, with the focus', async () => {
  let reads = 0
  const { bff, main } = await loadedServer({
    serverProfile: () => (++reads === 1 ? freeDoc() : usedDoc(TAKEN)),
    serverProfileDelete: () =>
      reply(409, {
        key: 'client-server-profile-in-use',
        args: [SERVER_ID, TAKEN],
      }),
  })
  const row = statusRow(main)
  expect(row, 'the status row').not.toBeNull()
  press(serverDeleteButton(main))
  const group = await within(main).findByRole('group', {
    name: SERVER_QUESTION,
  })
  expect(
    row!.nextElementSibling,
    '22 D2.8: the confirm opens under the row',
  ).toBe(group)
  expect(
    bff.to('DELETE', SERVER_ROUTE),
    'opening the confirm sends nothing',
  ).toHaveLength(0)
  press(confirmIn(group, 'confirm-delete'))
  expect(bff.to('DELETE', SERVER_ROUTE), 'the verb goes once').toHaveLength(1)
  const refusal = en('api-client-server-profile-in-use', SERVER_ID, TAKEN)
  await within(group).findByText(refusal)
  // The read again lands: the row names the instance, the minus and its group
  // go with it, and the alert must outlive them.
  await within(main).findByText(en('server-profile-used-by', TAKEN))
  expect(
    within(main).queryByRole('button', {
      name: en('server-profile-delete'),
    }),
    'the minus is gone',
  ).toBeNull()
  expect(
    within(main).queryByRole('group', { name: SERVER_QUESTION }),
    'and so is the confirm',
  ).toBeNull()
  const kept = within(main).getByText(refusal)
  expect(kept.tagName, 'the text is in a paragraph').toBe('P')
  expect(kept.getAttribute('role'), 'an alert').toBe('alert')
  expect(kept.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(
    row!.nextElementSibling,
    '22 D2.8: it comes after the row, not after the Value list',
  ).toBe(kept)
  expect(document.activeElement, 'and it has the focus').toBe(kept)
})

// ── 22 D2.9: the row wraps ──────────────────────────────────────────────

test('22 D2.9: the status row is a flex row that wraps with gap var(--space-3) and align-items center, its paragraph flex: 1 1 16rem and the minus flex: none, so the minus wraps under the status text at 320 px and 200 % text without leaving the row (the widths, both themes, and the bar clearing a focused control are the UX check states)', async () => {
  const { main } = await loaded()
  const row = statusRow(main)
  expect(row, 'the status row').not.toBeNull()
  const box = cascade(row as HTMLElement).decls
  expect(norm(box['display'] ?? ''), 'a flex row').toBe('flex')
  expect(norm(box['flex-wrap'] ?? ''), 'that wraps').toBe('wrap')
  expect(norm(box['gap'] ?? ''), 'with this space between the two').toBe(
    'var(--space-3)',
  )
  expect(norm(box['align-items'] ?? ''), 'centred on the line').toBe('center')
  expect(
    norm(cascade(row!.querySelector('p') as HTMLElement).decls['flex'] ?? ''),
    'the paragraph takes the line and gives way at 16rem',
  ).toBe('1 1 16rem')
  expect(
    norm(cascade(deleteButton(main)).decls['flex'] ?? ''),
    'the minus keeps its box',
  ).toBe('none')
})

// ── 22 D2.10: the strings ───────────────────────────────────────────────

test('22 D2.10: the strings keys of the slice are the words of the brief, and model-profile-delete-needs-off is gone (1a strings in App.test.tsx pins the strings file against this map)', () => {
  expect(
    EN['model-profile-on'],
    'the switched-on state, and where to change it',
  ).toBe(
    'This model profile is switched on: its engine serves it. To delete it, switch it off first in the list of all model profiles.',
  )
  expect(EN['server-profile-unused'], 'the server row, unused').toBe(
    'No instance uses this server profile.',
  )
  expect(EN['server-profile-used-by'], 'the server row, in use').toBe(
    'Used by instance {0}. To delete this server profile, give that instance another one first.',
  )
  expect(
    'model-profile-delete-needs-off' in EN,
    '22 D2.1: the key leaves the strings file',
  ).toBe(false)
})
