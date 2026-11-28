// Brief 26, slice IP6a (fe/briefs/26-instance-profiles.md, acceptance items
// 1 to 4): the nav's Profiles group with the instance profiles entry first
// (26.1), the instance profile list at `#/profiles/instance` (26.2: its
// columns, the model count, the Used by links, the loading, failed and empty
// states, and the Server profile as a muted second line under the name, the
// way 22 D4.35 folds the Engine), the row's trash can (26.3), and the create
// form (26.4). The wire is brief 26 section 0: GET, POST and DELETE
// /api/profiles/instance, answered by the shared fake (src/test/bff.ts) until
// the bff serves them (IP4). One profile's page, the entry page, the instance
// page and the server profile pages are later slices of the brief: nothing
// here reads them, and the fake answers the profile's own route with the
// manager's 404. The narrow-width folding of the Server profile column is the
// sheet's and the ux check's (`instance-profiles` state), as D4.35's is: the
// DOM always holds both copies, so only the copy inside the name cell is
// pinned here, and its muted look is read from the sheet the way
// L2ModelsList.test.tsx reads it.
//
// The IP6a review round (the brief's "IP6a review", 2026-11-28) is pinned here
// for what the DOM and the sheet show: the R4 words of
// `api-invalid-instance-profile-id` as a literal read of en_US.json, the muted
// second line's Model profiles count and that line's muted link (R2), the
// sheet's nav column (R1) and its fold of the Model profiles column (R2), the
// create form's check order and where its focus lands (26.4 CP.3), and the bad
// escape in the route. What is geometry stays in the ux check: the label on one
// line beside its icon (`nav-column` on `nav-open`), the words breaking only at
// a break point at 320 px and the line as it folds (`instance-profiles-narrow`
// on `instance-profiles`), and the in-use refusal of the row's trash
// (`instance-profiles-in-use`).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  arrived,
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  quiet,
  reply,
  startAt,
  type BffOptions,
  type InstanceProfile,
  type Instance,
  type ServerProfile,
} from './test/bff.ts'

const LIST = '/api/profiles/instance'

// The pool: `small` runs the `default` server profile and is used by the
// `default` instance, so its row's trash can cannot act; `svc` is free with
// two models; `solo` is free with none.
const POOL: InstanceProfile[] = [
  {
    id: 'small',
    server: 'default',
    models: ['Qwen3.5-9B'],
    usedBy: ['default'],
  },
  {
    id: 'svc',
    server: 'svc',
    models: ['Qwen3.5-9B', 'gpt-oss-120b'],
    usedBy: [],
  },
  { id: 'solo', server: 'svc', models: [], usedBy: [] },
]

const SERVERS: ServerProfile[] = [
  { id: 'default', tags: [] },
  { id: 'svc', tags: [] },
]

const TWO_BOXES: Instance[] = [
  { id: 'box-a', engine: 'llama.cpp', state: 'active', label: 'Box' },
  { id: 'box-b', engine: 'llama.cpp', state: 'inactive', label: 'Box' },
]

beforeEach(() => startAt('#/profiles/instance'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── The sheet, read as text (L2ModelsList.test.tsx's reader, a local copy) ──

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')
const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
}))
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}
const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

/** True when some rule of the sheet sets this property to this value on this
 * element or on one of its parents up to `stop`. */
const styled = (
  el: Element,
  name: string,
  value: string,
  stop: Element,
): boolean => {
  for (
    let node: Element | null = el;
    node !== null;
    node = node.parentElement
  ) {
    const hit = RULES.some((r) => {
      if (norm(prop(r.body, name)) !== norm(value)) return false
      try {
        return node!.matches(r.sel)
      } catch {
        return false
      }
    })
    if (hit) return true
    if (node === stop) return false
  }
  return false
}

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's timers settle a few tasks (the app's own effects). */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The app at the list page; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff({ instanceProfiles: POOL, ...options })
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** The app with the pool read answered: the table is on screen. */
async function loaded(options: BffOptions = {}) {
  const r = open(options)
  await within(r.main).findByRole('heading', {
    level: 1,
    name: en('instance-profiles-title'),
  })
  await waitFor(() =>
    expect(
      r.main.querySelector('table.instance-profiles'),
      'the table',
    ).not.toBeNull(),
  )
  return r
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The Profiles group's links: text, href, aria-current. */
function profileLinks() {
  const ul = nav().querySelector('ul[aria-labelledby="nav-profiles"]')
  expect(
    ul,
    'the Profiles group is a list labelled nav-profiles',
  ).not.toBeNull()
  return [...ul!.querySelectorAll<HTMLAnchorElement>('a')].map((a) => ({
    text: (a.textContent ?? '').trim(),
    href: a.getAttribute('href'),
    current: a.getAttribute('aria-current'),
  }))
}

function table(main: HTMLElement): HTMLElement {
  return main.querySelector<HTMLElement>('table.instance-profiles')!
}

/** The body rows, the confirm row left out. */
function dataRows(main: HTMLElement): HTMLElement[] {
  return [...table(main).querySelectorAll<HTMLElement>('tbody tr')].filter(
    (r) => r.closest('tr.confirm-row') === null,
  )
}

function confirmRow(main: HTMLElement): Element | null {
  return main.querySelector('tr.confirm-row')
}

/** The cells of a row. */
function cellsOf(row: HTMLElement): HTMLElement[] {
  return [...row.querySelectorAll<HTMLElement>('td')]
}

/** The row whose name cell is the profile id. 26.2 (the way 22 D4.35 folds the
 *  Engine) copies the Server profile into the name cell as a muted second line
 *  for the narrow page, so the row is matched on the id alone, not on the
 *  cell's whole text: the id `small` sits inside the Server profile
 *  `smalldefault`. */
function rowOf(main: HTMLElement, id: string): HTMLElement {
  const row = dataRows(main).find((r) => {
    const name = r.querySelector<HTMLElement>('td')
    return name !== null && cellText(name, true) === id
  })
  expect(row, `a row for ${id}`).not.toBeUndefined()
  return row!
}

/** The cells of that row. */
function cells(main: HTMLElement, id: string): HTMLElement[] {
  return cellsOf(rowOf(main, id))
}

/** A cell's text, the buttons' text dropped (a trash can names itself). The
 *  name cell is read as the id alone, its muted Server profile line left out
 *  (26.2, D4.35): that copy is the narrow page's, the second column pins it. */
function cellText(cell: HTMLElement, nameCell = false): string {
  const clone = cell.cloneNode(true) as HTMLElement
  for (const b of clone.querySelectorAll('button')) b.remove()
  if (nameCell) for (const m of clone.querySelectorAll('.muted')) m.remove()
  return (clone.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The cells' text, the buttons' text dropped (a trash can names itself). */
function rowTexts(main: HTMLElement): string[][] {
  return dataRows(main).map((row) =>
    cellsOf(row).map((c, i) => cellText(c, i === 0)),
  )
}

/** The list reads the bff was asked for, in order. */
function listReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', LIST)
}

/** The creates the bff was asked for, in order. */
function posts(bff: ReturnType<typeof fakeBff>) {
  return bff.to('POST', LIST)
}

/** The row's trash can, named by `instance-profiles-delete-row` with the id. */
function trashOf(main: HTMLElement, id: string): HTMLElement {
  return within(table(main)).getByRole('button', {
    name: en('instance-profiles-delete-row', id),
  })
}

const isActive = (el: HTMLElement): boolean =>
  el.getAttribute('aria-disabled') !== 'true'

const question = (id: string) => en('instance-profiles-delete-question', id)

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function alertTexts(root: Element): string[] {
  return [...root.querySelectorAll('[role="alert"]')]
    .map((a) => text(a))
    .filter((s) => s !== '')
}

/** The region's status line above the table (22 D4.6, shared by the delete). */
function regionStatus(main: HTMLElement): Element | null {
  const t = table(main)
  const line = [...main.querySelectorAll('[role="status"]')].find((s) =>
    before(s, t),
  )
  return line ?? null
}

/** The confirm of the trash can of `id`, opened. */
async function confirmGroup(
  main: HTMLElement,
  id: string,
): Promise<HTMLElement> {
  press(trashOf(main, id))
  return within(main).findByRole('group', { name: question(id) })
}

function confirmIn(group: HTMLElement, key: string): HTMLElement {
  return within(group).getByRole('button', { name: en(key) })
}

function groupStatus(group: HTMLElement): HTMLElement {
  const all = [...group.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the group has one status line').toHaveLength(1)
  return all[0]!
}

/** The opener of the create form, and the form. */
function newButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', {
    name: en('instance-profiles-new'),
  })
}

async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(newButton(main))
  return within(main).findByRole('form', {
    name: en('instance-profiles-new-title'),
  })
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

function selectIn(form: HTMLElement, label: string): HTMLSelectElement {
  return within(form).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function buttonIn(form: HTMLElement, key: string): HTMLElement {
  return within(form).getByRole('button', { name: en(key) })
}

/** The form's status line (17b.1: the form holds one). */
function formStatus(form: HTMLElement): HTMLElement {
  const all = [...form.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the form has one status line').toHaveLength(1)
  return all[0]!
}

/** What `el` says about itself at the moment the focus lands on it. */
function watchFocus(el: HTMLElement, form: HTMLElement, wanted: string) {
  const at = { focused: false, described: [] as string[], shown: false }
  el.addEventListener('focus', () => {
    at.focused = true
    at.described = (el.getAttribute('aria-describedby') ?? '')
      .split(/\s+/)
      .filter(Boolean)
      .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
    at.shown = (form.textContent ?? '').includes(wanted)
  })
  return at
}

/** Every tabbable, in order, by its text. */
function tabbableNames(): string[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => (el.textContent ?? '').replace(/\s+/g, ' ').trim())
}

// ── 26.1: the nav ───────────────────────────────────────────────────────

test('26.1: the Profiles group lists Instance profiles, Server profiles and Model profiles, in that order, each linking to its list, the entry aria-current page on its own list', async () => {
  const { main } = open()
  await within(main).findByRole('heading', {
    level: 1,
    name: en('instance-profiles-title'),
  })
  const links = profileLinks()
  expect(
    links.map((l) => [l.text, l.href]),
    'an instance profile composes the other two, so it comes first',
  ).toEqual([
    [en('nav-instance-profiles'), '#/profiles/instance'],
    [en('nav-server-profiles'), '#/profiles/server'],
    [en('nav-model-profiles'), '#/profiles/model'],
  ])
  expect(links[0]!.current, 'on its own list').toBe('page')
  expect(
    links[1]!.current,
    'Server profiles marks its own page only',
  ).toBeNull()
  expect(links[2]!.current, 'Model profiles marks its own page only').toBeNull()
})

test('26.1: on the Server profiles list the Instance profiles entry stands before it and marks nothing, and the Tab path carries it after Node info', async () => {
  startAt('#/profiles/server')
  const { main } = open({ serverProfiles: SERVERS })
  await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  // The nav's instances land a moment after the heading: settle the nav.
  await within(nav()).findByRole('link', { name: 'default' })
  const links = profileLinks()
  expect(links.map((l) => l.text)).toEqual([
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
  ])
  expect(links[0]!.current, 'the entry marks its own page only').toBeNull()
  expect(links[1]!.current).toBe('page')
  expect(
    tabbableNames().slice(0, 13),
    'the shell in 20 N.10 order with Instance profiles before Server profiles',
  ).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

// ── 26.2: the list ──────────────────────────────────────────────────────

test('26.2: #/profiles/instance is the list page: its h1 is instance-profiles-title and takes the focus, the intro stands under it, and it sends one GET /api/profiles/instance', async () => {
  const { bff, main } = open()
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('instance-profiles-title'),
  })
  // The focus and the document title are written by the heading's effects,
  // which run after the render that put the h1 in the tree (arrived).
  await arrived(h1, en('page-title', en('instance-profiles-title')))
  expect(h1.getAttribute('tabindex'), 'so it is focusable').toBe('-1')
  const intro = within(main).getByText(en('instance-profiles-intro'))
  expect(before(h1, intro), 'the intro is below the h1').toBe(true)
  await quiet(bff)
  expect(listReads(bff).length, 'one read of the pool').toBe(1)
})

test('26.2: the table has its five columns in order, one row per profile in the reply order, the name a link to the profile page and the Server profile a link to the server profile page', async () => {
  const { main } = await loaded()
  const t = table(main)
  expect(t, 'the list carries its class, as the other lists do').toBeTruthy()
  expect(
    [...t.querySelectorAll('thead th')].map((th) => text(th)),
    'the columns of 26.2, the shared Actions column last',
  ).toEqual([
    en('instance-profiles-column-name'),
    en('instance-profiles-column-server'),
    en('instance-profiles-column-models'),
    en('instance-profiles-column-used-by'),
    en('profiles-column-actions'),
  ])
  expect(
    rowTexts(main).map((r) => r[0]),
    'the rows in the reply order',
  ).toEqual(['small', 'svc', 'solo'])
  const [name, server] = cells(main, 'small')
  expect(
    within(name).getByRole('link', { name: 'small' }).getAttribute('href'),
    'the name is a link to the profile page',
  ).toBe('#/profiles/instance/small')
  expect(
    within(server).getByRole('link', { name: 'default' }).getAttribute('href'),
    'the Server profile is a link to its page',
  ).toBe('#/profiles/server/default')
})

test('26.2: the Model profiles cell counts the models, the one model with the singular key, and none with the muted instance-profiles-models-none', async () => {
  const { main } = await loaded()
  expect(text(cells(main, 'svc')[2]), 'two models').toBe(
    en('instance-profiles-models-count', '2'),
  )
  expect(
    text(cells(main, 'small')[2]),
    'one model: the singular key, no number',
  ).toBe(en('instance-profiles-models-count-one'))
  const none = cells(main, 'solo')[2]
  expect(text(none), 'no model').toBe(en('instance-profiles-models-none'))
  expect(
    styled(
      within(none).getByText(en('instance-profiles-models-none')),
      'color',
      'var(--muted)',
      none,
    ),
    'the word is muted (26.2)',
  ).toBe(true)
})

test('26.2: the Used by cell lists the instances that run the profile, comma-separated, as links to their pages named by the instance name, and a profile no instance runs says the muted No instance', async () => {
  const { main } = await loaded({
    instances: TWO_BOXES,
    instanceProfiles: [
      { id: 'pair', server: 'default', models: [], usedBy: ['box-a', 'box-b'] },
      { id: 'solo', server: 'svc', models: [], usedBy: [] },
    ],
  })
  const used = cells(main, 'pair')[3]
  const links = within(used).getAllByRole('link')
  expect(
    links.map((a) => a.getAttribute('href')),
    'one link per instance, in the reply order',
  ).toEqual(['#/instances/box-a', '#/instances/box-b'])
  expect(
    links.map((a) => text(a)),
    'each link is named by the instance name (D4.37)',
  ).toEqual(['Box', 'Box'])
  expect(
    text(used),
    'the cell reads the names comma-separated (the shared-name id is L2InstanceLink.test.tsx)',
  ).toContain('Box, Box')
  const free = cells(main, 'solo')[3]
  expect(within(free).queryAllByRole('link'), 'not a link').toHaveLength(0)
  expect(text(free), 'the free row says it').toBe(
    en('instance-profiles-unused'),
  )
  expect(
    styled(
      within(free).getByText(en('instance-profiles-unused')),
      'color',
      'var(--muted)',
      free,
    ),
    'the word is muted (26.2)',
  ).toBe(true)
})

test('26.2: while the read is open the page shows instance-profiles-loading, no table and no +, and the table lands when the read answers', async () => {
  const gate = deferred()
  const { main } = open({ instanceProfiles: () => gate.promise })
  // The shell's skeleton of U17a is what shows while the app boots, so the
  // page's own loading sentence is found, not read at once.
  expect(
    text(await within(main).findByText(en('instance-profiles-loading'))),
    'the loading sentence',
  ).toBe(en('instance-profiles-loading'))
  expect(
    main.querySelector('table.instance-profiles'),
    'no table while it is read',
  ).toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('instance-profiles-new') }),
    'no + while it is read',
  ).toBeNull()
  gate.resolve(reply(200, { rows: POOL }))
  await waitFor(() =>
    expect(
      main.querySelector('table.instance-profiles'),
      'the table',
    ).not.toBeNull(),
  )
})

test('26.2: 503 client-unavailable shows api-client-unavailable, and 500 and a network failure show instance-profiles-failed, each in place of the table and with no +', async () => {
  const cases: [string, () => Response | Promise<Response>][] = [
    [
      en('api-client-unavailable'),
      () => reply(503, { key: 'client-unavailable', args: [] }),
    ],
    [
      en('instance-profiles-failed'),
      () => reply(500, { key: 'client-other', args: [] }),
    ],
    [
      en('instance-profiles-failed'),
      () => Promise.reject(new TypeError('Failed to fetch')),
    ],
  ]
  for (const [wanted, answer] of cases) {
    const { main } = open({ instanceProfiles: answer })
    expect(text(await within(main).findByText(wanted)), wanted).toBe(wanted)
    expect(
      main.querySelector('table.instance-profiles'),
      'in place of the table',
    ).toBeNull()
    expect(
      within(main).queryByRole('button', { name: en('instance-profiles-new') }),
      'and no + after a failed read',
    ).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('26.2: an empty pool says instance-profiles-empty with no table, and the + stands after the sentence', async () => {
  // No `loaded`: an empty pool draws no table, so the empty sentence is what
  // settles the read.
  const { main } = open({ instanceProfiles: [] })
  const empty = await within(main).findByText(en('instance-profiles-empty'))
  expect(text(empty), 'the empty sentence').toBe(en('instance-profiles-empty'))
  expect(main.querySelector('table.instance-profiles'), 'no table').toBeNull()
  const opener = newButton(main)
  expect(before(empty, opener), 'the + is after the sentence').toBe(true)
})

test('22 D4.35 as 26.2 borrows it: each row carries its Server profile inside the name cell, under the name, as a muted link, and the Server profile column keeps its own cell beside it', async () => {
  const { main } = await loaded()
  const [name, server] = cells(main, 'small')
  const under = within(name).getByRole('link', { name: 'default' })
  expect(
    under.getAttribute('href'),
    'the second line names the server profile, as a link to it',
  ).toBe('#/profiles/server/default')
  expect(
    within(name).getByRole('link', { name: 'small' }),
    'the name link stays above it',
  ).toBeTruthy()
  expect(
    before(name, server),
    'the copy sits in the name cell, the column follows it',
  ).toBe(true)
  expect(
    styled(under, 'color', 'var(--muted)', name),
    'the second line is muted (D4.35)',
  ).toBe(true)
  expect(
    text(server),
    'at the wide widths the column reads as 26.2 spells it',
  ).toContain('default')
})

// ── 26.3: the row's trash can ───────────────────────────────────────────

test('26.3: every row has a trash can named instance-profiles-delete-row with its id; a row an instance runs is aria-disabled, keeps the focus, and names the tip instance-profiles-delete-in-use, a free row is active and names nothing, and a press of the inactive one sends and opens nothing', async () => {
  const { bff, main } = await loaded()
  const busy = trashOf(main, 'small')
  const free = trashOf(main, 'svc')
  expect(isActive(free), 'a free row can act').toBe(true)
  expect(
    free.getAttribute('aria-describedby'),
    'and needs no reason',
  ).toBeNull()
  expect(isActive(busy), 'an instance runs it: aria-disabled').toBe(false)
  expect(
    busy.getAttribute('tabindex'),
    'aria-disabled, not disabled: it stays focusable',
  ).not.toBe('-1')
  const said = document.getElementById(busy.getAttribute('aria-describedby')!)
  expect(said, 'the trash can names its tip').not.toBeNull()
  expect(text(said), 'the tip says what to do').toBe(
    en('instance-profiles-delete-in-use'),
  )
  press(busy)
  await flushTasks()
  expect(
    bff.calls.filter((c) => c.method === 'DELETE'),
    'a press of the inactive can sends nothing',
  ).toHaveLength(0)
  expect(confirmRow(main), 'and opens nothing').toBeNull()
})

test('26.3: the can of a free row opens a confirm row right after its row, a group named by instance-profiles-delete-question, Delete then Cancel, an empty status and no alert; the focus goes to Cancel, Cancel closes it, and Confirm sends one DELETE, removes the row and says instance-profiles-deleted above the table without reading the list again', async () => {
  const { bff, main } = await loaded()
  const reads = listReads(bff).length
  const row = rowOf(main, 'solo')
  const can = trashOf(main, 'solo')
  const group = await confirmGroup(main, 'solo')
  const tr = confirmRow(main)
  expect(tr, 'the confirm row stands in the table').not.toBeNull()
  expect(tr!.previousElementSibling, 'right after the profile row').toBe(row)
  const asked = document.getElementById(group.getAttribute('aria-labelledby')!)
  expect(text(asked), 'the question names the profile').toBe(question('solo'))
  expect(group.contains(asked), 'the question is in the group').toBe(true)
  expect(
    within(group)
      .getAllByRole('button')
      .map((b) => text(b)),
    'the verb then Cancel',
  ).toEqual([en('confirm-delete'), en('confirm-cancel')])
  expect(groupStatus(group).textContent, 'the status line is empty').toBe('')
  expect(alertTexts(group), 'the alert place is empty').toEqual([])
  expect(can.getAttribute('aria-expanded'), 'the can says it is open').toBe(
    'true',
  )
  const cancel = confirmIn(group, 'confirm-cancel')
  expect(document.activeElement, 'the focus is on Cancel').toBe(cancel)
  press(cancel)
  await flushTasks()
  expect(confirmRow(main), 'Cancel closes it').toBeNull()
  expect(document.activeElement, 'and the focus is back on the can').toBe(can)
  expect(can.getAttribute('aria-expanded'), 'closed').toBe('false')
  // The delete itself.
  const group2 = await confirmGroup(main, 'solo')
  press(confirmIn(group2, 'confirm-delete'))
  const done = await within(main).findByText(
    en('instance-profiles-deleted', 'solo'),
  )
  expect(bff.to('DELETE', `${LIST}/solo`).length, 'one DELETE').toBe(1)
  expect(
    rowTexts(main).map((r) => r[0]),
    'the row is gone, the others stay',
  ).toEqual(['small', 'svc'])
  expect(confirmRow(main), 'and so is the confirm row').toBeNull()
  expect(done.getAttribute('role'), 'a status line').toBe('status')
  expect(before(done, table(main)), 'above the table').toBe(true)
  expect(done, 'the region line, not a new one').toBe(regionStatus(main))
  expect(listReads(bff).length, 'the list was not read again').toBe(reads)
})

test('26.3: a 409 instance-profile-in-use keeps the row, reads the list again, puts api-instance-profile-in-use in the region alert above the table, leaves the focus on the can, and frees it once the re-read says an instance runs it', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    instanceProfiles: () =>
      reply(
        200,
        ++reads === 1
          ? { rows: POOL }
          : {
              rows: [
                POOL[0]!,
                POOL[1]!,
                { id: 'solo', server: 'svc', models: [], usedBy: ['default'] },
              ],
            },
      ),
    instanceProfileDelete: () =>
      reply(409, { key: 'instance-profile-in-use', args: ['default'] }),
  })
  const refusal = en('api-instance-profile-in-use', 'default')
  const group = await confirmGroup(main, 'solo')
  press(confirmIn(group, 'confirm-delete'))
  const kept = await within(main).findByText(refusal)
  await flushTasks()
  expect(listReads(bff).length, 'the list was read again').toBe(2)
  expect(confirmRow(main), 'the confirm row is gone').toBeNull()
  expect(kept.getAttribute('role'), 'the region alert').toBe('alert')
  expect(before(kept, table(main)), 'above the table').toBe(true)
  expect(
    rowTexts(main).map((r) => r[0]),
    'nothing was deleted',
  ).toEqual(['small', 'svc', 'solo'])
  const can = trashOf(main, 'solo')
  expect(isActive(can), 'the re-read says an instance runs it: disabled').toBe(
    false,
  )
  expect(document.activeElement, 'the focus is on the can').toBe(can)
})

// ── 26.4: the create form ───────────────────────────────────────────────

test('26.4: the + after the table opens a form in place named by its h2, with the focus on Name, the Name hint shown by its About toggle, a Server profile select of the pool ids led by instance-profile-server-choose, Create primary then Cancel, one empty status line and no alert, and Cancel closes and clears it', async () => {
  const { bff, main } = await loaded({ serverProfiles: SERVERS })
  const box = table(main).closest('div.table-scroll')
  expect(box, 'the table is in its scrolling box').not.toBeNull()
  const opener = newButton(main)
  expect(opener.getAttribute('aria-expanded'), 'a disclosure, closed').toBe(
    'false',
  )
  expect(before(box!, opener), 'the + is after the table').toBe(true)
  const form = await openForm(main)
  expect(opener.getAttribute('aria-expanded'), 'the press opens').toBe('true')
  const h2 = within(form).getByRole('heading', {
    level: 2,
    name: en('instance-profiles-new-title'),
  })
  expect(h2.getAttribute('id'), 'the form is named by its h2').toBe(
    form.getAttribute('aria-labelledby'),
  )
  expect(opener.nextElementSibling, 'the form is right after the +').toBe(form)
  const name = boxIn(form, en('instance-profile-name'))
  expect(document.activeElement, 'the focus moves to Name').toBe(name)
  const hint = within(form).getByText(en('instance-profile-name-hint'))
  expect(isHidden(hint), 'the hint starts closed (DESIGN 5.2)').toBe(true)
  press(
    within(form).getByRole('button', {
      name: `${en('field-about')} ${en('instance-profile-name')}`,
    }),
  )
  expect(isHidden(hint), 'About shows the hint').toBe(false)
  const select = selectIn(form, en('instance-profiles-column-server'))
  expect(
    [...select.querySelectorAll('option')]
      .map((o) => text(o))
      .filter((s) => s !== ''),
    'the pool by id, led by the choose option',
  ).toEqual([en('instance-profile-server-choose'), 'default', 'svc'])
  const create = buttonIn(form, 'create-button')
  expect(create.classList.contains('primary'), 'Create is the primary').toBe(
    true,
  )
  expect(before(name, create), 'Create is after the fields').toBe(true)
  expect(
    before(create, buttonIn(form, 'create-cancel')),
    'Cancel is after Create',
  ).toBe(true)
  expect(formStatus(form).textContent, 'the status line is empty at open').toBe(
    '',
  )
  expect(alertTexts(form), 'no alert: the place is empty').toEqual([])
  expect(posts(bff), 'opening sends nothing').toHaveLength(0)
  // Cancel: closed, cleared, focus back on the +.
  type(name, 'solo2')
  fireEvent.change(select, { target: { value: 'svc' } })
  press(buttonIn(form, 'create-cancel'))
  await flushTasks()
  expect(within(main).queryByRole('form'), 'the form is gone').toBeNull()
  expect(opener.getAttribute('aria-expanded'), 'the + is closed').toBe('false')
  expect(document.activeElement, 'the focus is on the +').toBe(opener)
  const again = await openForm(main)
  expect(
    boxIn(again, en('instance-profile-name')).value,
    'the field opens empty',
  ).toBe('')
  expect(
    selectIn(again, en('instance-profiles-column-server')).selectedIndex,
    'and the select is back on the choose option',
  ).toBe(0)
})

test('26.4: the checks answer in the brief order, each in the form status with the field aria-invalid, described by it, and the focus moved there after the words: an empty Name, a Name that is not a valid id, a Name the pool has letter case aside, and no server profile, and a page check sends nothing', async () => {
  const { bff, main } = await loaded({ serverProfiles: SERVERS })
  const form = await openForm(main)
  const status = formStatus(form)
  const name = boxIn(form, en('instance-profile-name'))
  const select = selectIn(form, en('instance-profiles-column-server'))
  const steps: [string, string, HTMLElement][] = [
    ['', en('field-error-required'), name],
    ['not a name!', en('api-invalid-instance-profile-id', 'not a name!'), name],
    ['SVC', en('api-instance-profile-name-taken', 'SVC'), name],
    ['solo2', en('field-error-choose'), select],
  ]
  for (const [value, wanted, field] of steps) {
    type(name, value)
    const at = watchFocus(field, form, wanted)
    press(buttonIn(form, 'create-button'))
    await flushTasks()
    expect(text(status), `the check says: ${wanted}`).toContain(wanted)
    expect(
      field.getAttribute('aria-invalid'),
      'the field is aria-invalid',
    ).toBe('true')
    expect(at.focused, 'the focus moved there').toBe(true)
    expect(at.shown, 'the focus moved after the words rendered').toBe(true)
    expect(
      at.described.some((d) => d.includes(wanted)),
      'the field is described by the words',
    ).toBe(true)
  }
  await quiet(bff)
  expect(posts(bff), 'a page check sends nothing').toHaveLength(0)
})

test('26.4: Create says create-creating while it runs, a 201 sends one POST of the id and the server profile and lands on the new profile at #/profiles/instance/<id> with its h1 focused', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfiles: SERVERS,
    instanceProfileCreate: () => gate.promise,
  })
  const form = await openForm(main)
  type(boxIn(form, en('instance-profile-name')), 'solo2')
  fireEvent.change(selectIn(form, en('instance-profiles-column-server')), {
    target: { value: 'svc' },
  })
  press(buttonIn(form, 'create-button'))
  await flushTasks()
  expect(text(formStatus(form)), 'the form says it is creating').toContain(
    en('create-creating'),
  )
  gate.resolve(reply(201, { id: 'solo2' }))
  await waitForHash('#/profiles/instance/solo2')
  const posts = bff.to('POST', LIST)
  expect(posts.length, 'one create').toBe(1)
  expect(posts[0]!.body, 'the id and the server profile').toEqual({
    id: 'solo2',
    server: 'svc',
  })
  const h1 = screen.getByRole('heading', { level: 1, name: 'solo2' })
  await waitFor(() =>
    expect(document.activeElement, 'its h1 takes the focus').toBe(h1),
  )
})

test('26.4: a 409 instance-profile-name-taken answers in the form alert naming the profile, keeps the form open on the list, and sends no second create', async () => {
  const { bff, main } = await loaded({
    serverProfiles: SERVERS,
    instanceProfileCreate: () =>
      reply(409, { key: 'instance-profile-name-taken', args: ['solo2'] }),
  })
  const form = await openForm(main)
  type(boxIn(form, en('instance-profile-name')), 'solo2')
  fireEvent.change(selectIn(form, en('instance-profiles-column-server')), {
    target: { value: 'svc' },
  })
  press(buttonIn(form, 'create-button'))
  const alert = await within(form).findByText(
    en('api-instance-profile-name-taken', 'solo2'),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the refusal is an alert',
  ).not.toBeNull()
  expect(window.location.hash, 'the page stays on the list').toBe(
    '#/profiles/instance',
  )
  expect(
    within(main).queryByRole('form', {
      name: en('instance-profiles-new-title'),
    }),
    'the form stays open',
  ).not.toBeNull()
  await quiet(bff)
  expect(bff.to('POST', LIST).length, 'one create, no retry').toBe(1)
})

test('26.4 (the IP6a review CP.3 order): with an empty server profile pool the select is replaced by instance-profile-no-server with a link to the server profiles, Create stays, an empty Name is answered the required check FIRST, and nothing is sent', async () => {
  const { bff, main } = await loaded({ serverProfiles: [] })
  const form = await openForm(main)
  expect(
    within(form).queryByRole('combobox'),
    'no select to choose from',
  ).toBeNull()
  const note = within(form).getByText(en('instance-profile-no-server'), {
    selector: 'p',
  })
  const link = within(note).getByRole('link', {
    name: en('nav-server-profiles'),
  })
  expect(
    link.getAttribute('href'),
    'the note links to the server profiles',
  ).toBe('#/profiles/server')
  // 26.4 lists the checks in order and the empty Name is first of them, so an
  // empty pool does not answer before it: the pool is what the LAST check needs.
  const name = boxIn(form, en('instance-profile-name'))
  const at = watchFocus(name, form, en('field-error-required'))
  press(buttonIn(form, 'create-button'))
  await flushTasks()
  expect(
    text(formStatus(form)),
    'the empty Name is what Create answers, not the empty pool',
  ).toContain(en('field-error-required'))
  expect(name.getAttribute('aria-invalid'), 'Name is the field at fault').toBe(
    'true',
  )
  expect(at.focused, 'the focus moves to Name').toBe(true)
  await quiet(bff)
  expect(posts(bff), 'a page check sends nothing').toHaveLength(0)
})

test('26.4 (the IP6a review CP.3 order): with the pool empty and a Name that is a valid id, Create fails the choose check and moves the focus to the Server profiles link of the note, the control that answers the check, and sends nothing', async () => {
  const { bff, main } = await loaded({ serverProfiles: [] })
  const form = await openForm(main)
  type(boxIn(form, en('instance-profile-name')), 'solo2')
  const link = within(form).getByRole('link', {
    name: en('nav-server-profiles'),
  })
  const at = watchFocus(link, form, en('field-error-choose'))
  press(buttonIn(form, 'create-button'))
  await flushTasks()
  expect(text(formStatus(form)), 'Create fails the choose check').toContain(
    en('field-error-choose'),
  )
  expect(
    at.focused,
    'there is no select to focus: the focus goes to the link the note offers',
  ).toBe(true)
  await quiet(bff)
  expect(posts(bff), 'a page check sends nothing').toHaveLength(0)
})

// ── The IP6a review (fe/briefs/26-instance-profiles.md, 2026-11-28) ──────

test('IP6a review R4: en_US.json says api-invalid-instance-profile-id in sentence case, with a full stop and no backticks, the shape of api-invalid-model-profile-id', () => {
  const table: Record<string, string> = strings
  expect(table['api-invalid-instance-profile-id']).toBe(
    "{0} cannot be an instance profile name: use letters, digits, '.', '_' and '-', starting with a letter or a digit.",
  )
})

test('IP6a review R2: each row’s muted second line under the name carries the server profile link, the separator and the Model profiles count, and its link takes --muted so a row shows one purple link', async () => {
  const { main } = await loaded()
  const wanted: [string, string][] = [
    // id, the Model profiles cell's text of 26.2, the singular and the none too.
    ['small', en('instance-profiles-models-count-one')],
    ['svc', en('instance-profiles-models-count', '2')],
    ['solo', en('instance-profiles-models-none')],
  ]
  for (const [id, count] of wanted) {
    const [name, server, models] = cells(main, id)
    const line = name.querySelector('.muted')
    expect(line, `the name cell of ${id} has a second line`).not.toBeNull()
    const links = [...line!.querySelectorAll('a')]
    expect(links.length, `${id}: the line holds the server profile alone`).toBe(
      1,
    )
    expect(
      links[0]!.getAttribute('href'),
      `${id}: it is a link to the server profile`,
    ).toBe(`#/profiles/server/${text(server)}`)
    expect(
      text(line!),
      `${id}: the line is the Server profile and the Model profiles column, in words`,
    ).toBe(`${text(server)} · ${count}`)
    expect(count, `${id}: the count is the column's own text`).toBe(
      text(models),
    )
    expect(
      styled(links[0]!, 'color', 'var(--muted)', links[0]!),
      `${id}: the link takes --muted, not the purple of a link (R2)`,
    ).toBe(true)
  }
})

test('IP6a review R1: the sheet gives the nav column 248 px, so "Instance profiles" stands on one line beside its icon', () => {
  // `.layout`'s grid columns, as the sheet writes them in the wide block (the
  // rule reader leaves the `@media` on the first rule of a block and gives the
  // ones after it their own selector).
  const columns = RULES.filter((r) => r.sel === '.layout')
    .map((r) => norm(prop(r.body, 'grid-template-columns')))
    .filter((v) => v !== '')
  expect(columns, 'the wide nav column').toEqual(['248px minmax(0, 1fr)'])
})

test('IP6a review R2: below 480 px the Model profiles column folds into the name cell the way the Server profile column does, its cell emptied and its head hidden the way .visually-hidden hides', () => {
  const on = (sel: string, name: string) =>
    RULES.filter((r) => r.sel === sel)
      .map((r) => norm(prop(r.body, name)))
      .filter((v) => v !== '')
  for (const n of ['2', '3']) {
    expect(
      on(`.instance-profiles td:nth-child(${n})`, 'display'),
      `the cell of column ${n} is out of the layout`,
    ).toContain('none')
    expect(
      on(`.instance-profiles tbody td:nth-child(${n})`, 'visibility'),
      `column ${n} keeps a box, emptied, so every head stands over its column`,
    ).toContain('hidden')
    expect(
      on(`.instance-profiles th:nth-child(${n})`, 'clip-path'),
      `the head of column ${n} is hidden the way .visually-hidden hides`,
    ).toContain('inset(50%)')
  }
})

test('26.2: a bad escape in the id of the route (#/profiles/instance/%zz) is the not-found page, and no read is sent', async () => {
  startAt('#/profiles/instance/%zz')
  const bff = fakeBff({
    instanceProfiles: POOL,
    instanceProfile: () => reply(200, {}),
  })
  render(<App />)
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('not-found'),
  })
  expect(document.activeElement, 'the not-found h1 takes the focus').toBe(h1)
  expect(
    bff.calls.filter((c) => c.path.startsWith('/api/profiles/instance/')),
    'a name no read can answer is never read',
  ).toHaveLength(0)
})

/** The hash, once the app has moved the page there. */
async function waitForHash(hash: string) {
  await waitFor(() => expect(window.location.hash).toBe(hash))
}
