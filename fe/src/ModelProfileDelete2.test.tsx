// Brief 22, slice 22D2 (fe/briefs/22-model-sources.md, section 22D2): the delete
// of a model profile becomes a minus icon button in a new status row under the
// summary line, the server profile page follows with the same row and its
// "used by" status, and the Save bar moves to the top of both pages, sticky at
// the viewport's top. The answers of 22 D.1 to 22 D.7 (ModelProfileDelete.test.tsx)
// and of 16 17b.6 to 17b.9 (ServerProfilePage2.test.tsx) stand: what this slice
// changes there is the place of the opener, of the muted line, of the button
// after the Value list and of the bar in the sheet, as 22 D2.10 lists.
//
// Slice 22D4 (the same brief, section 22D4) takes the status row away on both
// pages: the − moves to the list rows and the page's Delete to the Save bar,
// which 22D2 put at the top and kept. So D2.1 to D2.4 and D2.8 to D2.10 name
// the bar and the plain status paragraph here, and the row's flex row is the
// list cell's (ModelProfilesDelete1.test.tsx and ServerProfilesDelete1.test.tsx).
// D2.5 to D2.7 are the bar, which 22D4 keeps.
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

/** The save bar's button group: 22 D4.10 puts the page's Delete first in it. */
function barActions(main: HTMLElement): HTMLElement {
  const actions = main.querySelector('.save-actions')
  expect(actions, 'the bar’s button group of 2h.1').not.toBeNull()
  return actions as HTMLElement
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

function queryServerConfirm(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('group', { name: SERVER_QUESTION })
}

// ── The stylesheet reader (22 D2.2, D2.6, D2.9) ─────────────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  compact: string
  sel: string
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
  compact: m[1].replace(/\s+/g, ''),
  // The selector as written: 22 D4.10 pins a gap the sheet gives the bar's
  // Delete, which needs the descendant and child combinators `compact` drops.
  sel: m[1].trim(),
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

/** Does the rule's selector name `el`, as written (the combinators kept)? */
const names = (rule: Rule, el: HTMLElement): boolean =>
  rule.sel
    .split(',')
    .map((s) => s.trim())
    .some((s) => {
      try {
        return el.matches(s)
      } catch {
        // A selector jsdom's engine does not know (a :has() the sheet uses):
        // it cannot name this element, so it is not the rule the pin looks for.
        return false
      }
    })

/** The top-level rules that give `el` a `--space-5` gap on its right, the
 *  properties that put space after a thing in a row (22 D4.10). The selector is
 *  the coder's: the pin is that one rule names this button on the page and
 *  declares the gap, and that the gap is the Delete's own. */
const GAP_RIGHT = ['margin-right', 'margin-inline-end']
const gapRules = (el: HTMLElement): Rule[] =>
  TOP.filter(
    (r) =>
      GAP_RIGHT.some((p) => norm(prop(r.body, p)) === 'var(--space-5)') &&
      names(r, el),
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

test('22 D2.1 (as 22 D4.9 amends it): the status row is gone; a plain p right after the summary line says the state, above the settings section, and the Save bar’s Delete is its first button in both states, switched off when the profile is switched on', async () => {
  const { main } = await loaded()
  const summary = main.querySelector('p.model-profile-summary')
  expect(summary, 'the summary line is there').not.toBeNull()
  expect(
    main.querySelector('div.page-status'),
    '22 D4.9: the status row of 22 D2.1 is gone',
  ).toBeNull()
  const para = summary!.nextElementSibling as HTMLElement | null
  expect(para, '22 D4.9: a paragraph follows the summary line').not.toBeNull()
  expect(para!.tagName, 'a plain paragraph, not a row of controls').toBe('P')
  expect(
    para!.classList.contains('page-status'),
    'and it carries no row class of its own',
  ).toBe(false)
  expect(
    text(para),
    'a switched-off profile says it is off, the words of 22D',
  ).toBe(en('model-profile-off'))
  const h2 = settingsTitle(main)
  expect(before(para!, h2), 'and it sits above the settings section').toBe(true)
  expect(
    para!.closest('section'),
    'above the section, so full width',
  ).toBeNull()
  const bar = saveBar(main)
  expect(bar, 'the save bar is there').not.toBeNull()
  const button = deleteButton(main)
  expect(
    button.closest('.save-actions'),
    '22 D4.10: the delete is in the bar, not on the page',
  ).toBe(barActions(main))
  expect(
    button.getAttribute('aria-disabled'),
    'a switched-off profile: the delete can act',
  ).not.toBe('true')
  expect(
    within(main).queryByText(NEEDS_OFF),
    'a switched-off profile says nothing about switching it off',
  ).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // A switched-on profile: the paragraph says what the state is and where to
  // change it (the words replace the muted line of 22 D.1), and the bar's
  // Delete stays, switched off (22 D4.10: never hidden).
  const on = await loaded({ modelProfile: () => onDoc() })
  const onPara = on.main.querySelector('p.model-profile-summary')!
    .nextElementSibling as HTMLElement | null
  expect(text(onPara), '22 D4.9: the paragraph says model-profile-on').toBe(
    en('model-profile-on'),
  )
  const kept = within(on.main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    kept.getAttribute('aria-disabled'),
    '22 D4.10: a switched-on profile keeps the Delete, switched off',
  ).toBe('true')
  expect(
    within(on.main).queryByText(NEEDS_OFF),
    '22 D2.1: the muted line is gone, its key leaves the strings file',
  ).toBeNull()
})

// ── 22 D2.2: the minus button ───────────────────────────────────────────

test('22 D2.2 (as 22 D4.10 amends it): the page’s Delete is a text button type="button" with id model-profile-delete, its words written on it and no icon, a secondary button of the bar’s group, and the sheet gives it a --space-5 gap after it and not to Discard or Save changes', async () => {
  const { main } = await loaded()
  const button = deleteButton(main)
  expect(button.tagName, 'a native button').toBe('BUTTON')
  expect(button.getAttribute('type'), 'it submits nothing').toBe('button')
  expect(button.id, '22 D2.2: the id the UX check finds it by').toBe(
    'model-profile-delete',
  )
  expect(
    text(button),
    '22 D4.10: a text button: the words are on it, so no hidden name is needed',
  ).toBe(en('model-profile-delete'))
  expect(
    button.querySelector('svg'),
    '22 D4.10: the glyph is the list’s, not the page’s',
  ).toBeNull()
  expect(
    button.querySelector('span.visually-hidden'),
    'and there is no hidden name',
  ).toBeNull()
  expect(
    button.className,
    '22 D4.10: a secondary text button, so Save changes stays the bar’s primary',
  ).toContain('secondary')
  const actions = barActions(main)
  expect(button.closest('.save-actions'), 'in the bar’s group').toBe(actions)
  expect(
    [...actions.children].indexOf(button),
    '22 D4.10: first, before Discard and Save changes',
  ).toBe(0)
  // The gap: one rule, naming this button, putting --space-5 after it.
  const gaps = gapRules(button)
  expect(
    gaps.length,
    '22 D4.10: exactly one rule gives the bar’s Delete a --space-5 gap on its right',
  ).toBe(1)
  for (const key of ['save-discard', 'save-button']) {
    const other = within(actions).getByRole('button', { name: en(key) })
    expect(
      gapRules(other).length,
      `22 D4.10: the gap is the Delete's own, so ${key} carries no --space-5 of its own`,
    ).toBe(0)
  }
})

// ── 22 D2.3: the confirm opens under the row ────────────────────────────

test('22 D2.3 (as 22 D4.11 amends it): pressing the bar’s Delete opens the confirm group inside the bar, under its buttons; the focus moves to Cancel, Cancel and Escape put it back on the Delete, and nothing is sent', async () => {
  const { bff, main } = await loaded()
  const opener = deleteButton(main)
  const group = await confirmGroup(main)
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  const bar = saveBar(main)
  expect(bar, 'the save bar of 22 D2.5').not.toBeNull()
  expect(
    group.closest('.save-bar'),
    '22 D4.11: the group is in the bar, which grows for it',
  ).toBe(bar)
  const actions = barActions(main)
  expect(
    before(actions, group),
    '22 D4.11: and under the bar’s buttons, not above them',
  ).toBe(true)
  expect(
    group.closest('.save-actions'),
    '22 D4.11: not inside the button group, which is a row of buttons',
  ).toBeNull()
  const cancel = confirmIn(group, 'confirm-cancel')
  expect(document.activeElement, '22 D.2: the focus is on Cancel').toBe(cancel)
  expect(
    bff.to('DELETE', ROUTE),
    'nothing is sent before the verb',
  ).toHaveLength(0)
  press(cancel)
  expect(queryConfirm(main), 'Cancel closes it').toBeNull()
  expect(document.activeElement, 'and the focus is back on the Delete').toBe(
    opener,
  )
  const again = await confirmGroup(main)
  expect(document.activeElement, 'Cancel again').toBe(
    confirmIn(again, 'confirm-cancel'),
  )
  fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' })
  expect(queryConfirm(main), 'Escape closes it').toBeNull()
  expect(document.activeElement, 'on the Delete').toBe(opener)
  expect(bff.to('DELETE', ROUTE), 'still nothing sent').toHaveLength(0)
})

// ── 22 D2.4: switched on meanwhile ──────────────────────────────────────

test('22 D2.4 (as 22 D4.11 amends it): after the 409 model-profile-enabled and the read again the paragraph says model-profile-on, the bar’s Delete stays switched off with the focus on it, and p#model-profile-delete-refused[role="alert"][tabindex="-1"] is the bar’s alert', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    modelProfile: () => (++reads === 1 ? offDoc() : onDoc()),
    modelProfileDelete: () =>
      reply(409, { key: 'model-profile-enabled', args: [ID] }),
  })
  const refusal = en('api-model-profile-enabled', ID)
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  await within(main).findByText(refusal)
  // The read again lands: the paragraph says the profile is on, the group goes
  // with it, and the alert must outlive them: it is the bar's.
  await within(main).findByText(en('model-profile-on'))
  expect(queryConfirm(main), 'the confirm is gone').toBeNull()
  expect(readCount(bff), 'the document was read again').toBe(2)
  const kept = within(main).getByText(refusal)
  expect(kept.tagName, 'the text is in a paragraph').toBe('P')
  expect(kept.id, '22 D2.4: the id 22D gives it').toBe(
    'model-profile-delete-refused',
  )
  expect(kept.getAttribute('role'), 'an alert').toBe('alert')
  expect(kept.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(
    kept.closest('.save-bar'),
    '22 D4.11: it is the bar’s alert, the place the bar keeps for a refusal',
  ).toBe(saveBar(main))
  const del = deleteButton(main)
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.11: the Delete stays switched off, the page says why',
  ).toBe('true')
  expect(
    document.activeElement,
    '22 D4.11: the focus moves to the Delete',
  ).toBe(del)
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

test('22 D2.7 (as 22 D4.11 amends it): Tab reaches the back link, the bar’s Delete, Discard and Save changes, the jump list and then the fields; with the confirm open its two buttons come after the bar’s three, and the bar’s buttons come before every field of the cards', async () => {
  const { main } = await loaded()
  const stops = mainTabbable(main)
  expect(
    stops.slice(0, 4),
    '22 D4.10: back link, then the bar: its Delete, Discard, Save changes',
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
  // The confirm, open, is the pair right after the bar's own buttons, since
  // 22 D4.11 puts the group inside the bar, under them.
  const off = await loaded()
  const group = await confirmGroup(off.main)
  const open = mainTabbable(off.main)
  expect(
    open.slice(1, 6),
    '22 D4.11: the bar’s Delete, Discard, Save changes, then the group’s verb and Cancel',
  ).toEqual([
    en('model-profile-delete'),
    en('save-discard'),
    en('save-button'),
    en('confirm-delete'),
    en('confirm-cancel'),
  ])
  expect(
    open.indexOf(en('model-profile-jump-label')),
    'the jump list comes after the open group',
  ).toBeGreaterThan(open.indexOf(en('confirm-cancel')))
  expect(
    group.contains(document.activeElement as Node),
    'the focus is inside the group, on Cancel',
  ).toBe(true)
})

// ── 22 D2.8: the server profile page follows ────────────────────────────

test('22 D2.8 (as 22 D4.9 and 22 D4.10 amend it): the server page’s status row is gone and the Value list’s Used by row stays; the Save bar’s Delete is its first button, active when no instance uses the profile, and switched off with the list’s reason when one does', async () => {
  const { main } = await loadedServer()
  expect(
    main.querySelector('div.page-status'),
    '22 D4.9: the status row of 22 D2.8 is gone',
  ).toBeNull()
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list is there').not.toBeNull()
  expect(
    text(dl!.querySelector('dt')),
    '22 D2.8: the Value list keeps its Used by row, which is data',
  ).toBe(en('server-profiles-column-used-by'))
  const button = serverDeleteButton(main)
  expect(button.id, '22 D4.10: the id the UX check finds it by').toBe(
    'server-profile-delete',
  )
  expect(button.getAttribute('type'), 'it submits nothing').toBe('button')
  expect(text(button), '22 D4.10: a text button: the words are on it').toBe(
    en('server-profile-delete'),
  )
  expect(button.querySelector('svg'), 'and no glyph').toBeNull()
  expect(
    button.className,
    '22 D4.10: secondary, so Save changes stays the bar’s primary',
  ).toContain('secondary')
  const actions = barActions(main)
  expect(button.closest('.save-actions'), 'in the bar’s group').toBe(actions)
  expect(
    [...actions.children].indexOf(button),
    '22 D4.10: first, before Discard and Save changes',
  ).toBe(0)
  expect(
    button.getAttribute('aria-disabled'),
    '22 D4.10: no instance uses it, so the Delete can act',
  ).not.toBe('true')
  cleanup()
  vi.unstubAllGlobals()
  // A profile an instance runs: the same button, switched off with the list's
  // reason as its description (22 D4.10).
  const used = await loadedServer({ serverProfile: () => usedDoc(TAKEN) })
  const kept = within(used.main).getByRole('button', {
    name: en('server-profile-delete'),
  })
  expect(
    kept.getAttribute('aria-disabled'),
    '22 D4.10: a profile in use keeps the Delete, switched off',
  ).toBe('true')
  const describedBy = kept.getAttribute('aria-describedby')
  expect(describedBy, 'and it points at its reason').toBeTruthy()
  const reason = used.main.querySelector(
    `#${describedBy}`,
  ) as HTMLElement | null
  expect(reason, 'the reason is on the page').not.toBeNull()
  expect(text(reason), '22 D4.10: the list’s reason, shared').toBe(
    en('server-profiles-delete-in-use'),
  )
  press(kept)
  expect(
    within(used.main).queryByRole('group', { name: SERVER_QUESTION }),
    'a press of the switched-off Delete opens nothing',
  ).toBeNull()
})

test('22 D2.8 (as 22 D4.11 amends it): on the server page the confirm opens inside the Save bar, under its buttons, and after the 409 client-server-profile-in-use and the read again its paragraph is the bar’s alert, with the Delete left switched off and the focus on it', async () => {
  let reads = 0
  const { bff, main } = await loadedServer({
    serverProfile: () => (++reads === 1 ? freeDoc() : usedDoc(TAKEN)),
    serverProfileDelete: () =>
      reply(409, {
        key: 'client-server-profile-in-use',
        args: [SERVER_ID, TAKEN],
      }),
  })
  press(serverDeleteButton(main))
  const group = await within(main).findByRole('group', {
    name: SERVER_QUESTION,
  })
  const bar = saveBar(main)
  expect(bar, 'the save bar').not.toBeNull()
  expect(
    group.closest('.save-bar'),
    '22 D4.11: the confirm is in the bar, which grows for it',
  ).toBe(bar)
  expect(
    before(barActions(main), group),
    '22 D4.11: and under the bar’s buttons',
  ).toBe(true)
  expect(
    bff.to('DELETE', SERVER_ROUTE),
    'opening the confirm sends nothing',
  ).toHaveLength(0)
  press(confirmIn(group, 'confirm-delete'))
  expect(bff.to('DELETE', SERVER_ROUTE), 'the verb goes once').toHaveLength(1)
  const refusal = en('api-client-server-profile-in-use', SERVER_ID, TAKEN)
  await within(main).findByText(refusal)
  // The read again lands: the group goes with the in-use state, and the alert
  // must outlive it: it is the bar's.
  expect(queryServerConfirm(main), 'the confirm is gone').toBeNull()
  expect(
    bff.to('GET', SERVER_ROUTE).length,
    '22 D4.11: the server page was read again',
  ).toBe(2)
  const kept = within(main).getByText(refusal)
  expect(kept.tagName, 'the text is in a paragraph').toBe('P')
  expect(kept.getAttribute('role'), 'an alert').toBe('alert')
  expect(kept.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(
    kept.closest('.save-bar'),
    '22 D4.11: it is the bar’s alert, the place the bar keeps for a refusal',
  ).toBe(bar)
  const del = serverDeleteButton(main)
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.11: the Delete stays switched off, the page says why',
  ).toBe('true')
  expect(
    document.activeElement,
    '22 D4.11: the focus moves to the Delete',
  ).toBe(del)
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
})

// ── 22 D2.9: the row wraps ──────────────────────────────────────────────

test('22 D2.9 (as 22 D4.9 moves it): the page’s status row is gone, so nothing of the delete wraps on the page, and the page’s Delete is a text button, not the 40 px square; the wrapping flex row is now the list cell’s, pinned by 22 D4.1 in ModelProfilesDelete1.test.tsx (the widths, both themes, and the bar clearing a focused control are the UX check states)', async () => {
  const { main } = await loaded()
  expect(
    main.querySelector('div.page-status'),
    '22 D4.9: the row that wrapped is gone',
  ).toBeNull()
  const button = deleteButton(main)
  expect(button.querySelector('svg'), 'a text button, no glyph').toBeNull()
  expect(
    norm(cascade(button).decls['width'] ?? ''),
    '22 D4.10: it takes its width from its words, not the list’s 40 px square',
  ).not.toBe('40px')
})

// ── 22 D2.10: the strings ───────────────────────────────────────────────

test('22 D2.10 (as 22 D4.12 amends it): the strings keys of the slices are the words of the brief: model-profile-on ends with the short sentence, server-profile-unused and server-profile-used-by leave the strings file, and model-profile-delete-needs-off is gone (1a strings in App.test.tsx pins the strings file against this map; the keys 22D4 gains are pinned by 22 D4.12 in ModelProfilesDelete1.test.tsx)', () => {
  expect(
    EN['model-profile-on'],
    '22 D4.9: the switched-on state, and where to change it',
  ).toBe(
    'This model profile is switched on: its engine serves it. To delete it, switch it off first.',
  )
  expect(
    'server-profile-unused' in EN,
    '22 D4.12: the server row’s unused sentence leaves the strings file',
  ).toBe(false)
  expect(
    'server-profile-used-by' in EN,
    '22 D4.12: and so does the in-use one',
  ).toBe(false)
  expect(
    'model-profile-delete-needs-off' in EN,
    '22 D2.1: the muted line’s key is gone',
  ).toBe(false)
})
