// Brief 22, slice L2 (fe/briefs/22-model-sources.md, "L2 acceptance list for
// the tester" items 1, 2, 3 and 5, with items 1 and 2 read as the user's
// L2 amendments B1 and B2 of 2026-11-26): the Save bar at the end of the form
// and sticky at the bottom edge (L2.B1), the reason of a Delete that cannot act
// as the list rows' floating tip and not a line of the bar (L2.B2), the bar's
// order (22 D4.30 with L2.A2: Discard, Save changes, Delete pushed to the end
// in --danger), the four confirm verbs of CD.1, and CD.3 (the confirms' texts,
// order and focus, unchanged). The pages' own read and save are
// ModelProfileSave*.test.tsx and ServerSave.test.tsx, the deletes' flows
// ModelProfileDelete*.test.tsx and ServerProfilePage2.test.tsx; the sheet rules
// L2.B1 gives the bar (sticky bottom, the border and corners on the top edge,
// the scroll padding and margin moved to the bottom, the short-viewport rule
// flipped) and CD.2 gives `.primary.danger` are in Reflow.test.ts, beside the
// other CSS readers.
//
// The round's second review of the same day (the brief's "D4, the page Delete
// is empty below 480 px") withdraws advice 1, the short verb of the narrow bar,
// so its test is gone: the Save bar's Delete has one label, the full verb, and
// the pins of that one label sit at the end of this file.
//
// Not provable in jsdom, so not tested here: the last card's bottom lying at or
// above the bar's top when the page is scrolled to its end (L2.B1 c), the bar
// at most 40 % of a 320 by 568 viewport with its buttons wrapped and no
// sideways scroll (L2.B1 e), and the jump list leaving the card heading whole
// at the top (L2.B1 f): jsdom lays nothing out, so the designer measures those
// with the UX check's page states. What is pinned here is the DOM (the bar the
// section's last child, its stops the page's last, the reason an element of the
// bar's Delete that is not shown) and, with the rects stubbed as
// ListDeleteTip.test.tsx stubs them, the tip's place at the viewport's bottom
// edge (L2.B2 c: above Delete, its right edge at Delete's).
//
// The bar's own rules (22 D4.10 to D4.14) are not re-pinned in full: the ids,
// aria-expanded and the description are pinned as L2 item 1 states them, "the
// same as 22 D4.10 to D4.14", so a move of the button that drops one of them
// is red here.
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
  accName,
  before,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  serverProfileDoc,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

// ── The sheet, read as text (Reflow.test.ts's reader, a small local copy:
// importing that file would register its tests here) ───────────────────────
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')
const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
  at: m.index ?? 0,
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

/** Every rule of the sheet that reaches `el`. A selector jsdom's engine cannot
 *  parse is skipped (a `:has()` of another element, a pseudo-element). */
const reaching = (el: Element): typeof RULES =>
  RULES.filter((r) => {
    try {
      return el.matches(r.sel)
    } catch {
      return false
    }
  })

/** Every rule that reaches `el` and declares `name`, as "selector: name:
 *  value", so a red names the rule to fix. */
const declaring = (el: Element, name: string): string[] =>
  reaching(el)
    .filter((r) => prop(r.body, name) !== '')
    .map((r) => `${r.sel}: ${name}: ${norm(prop(r.body, name))}`)

/** True when some rule of the sheet sets this property to this value on this
 * element. The selector is not named: D4.30 and CD.1 say what the control
 * looks like, not what class carries it, so the sheet is free to reach the
 * element by class, by id or by its place in the bar. A selector jsdom's
 * engine cannot parse is skipped (a `:has()` of another element, a
 * pseudo-element). */
const styled = (el: Element, name: string, value: string): boolean =>
  RULES.some((r) => {
    if (norm(prop(r.body, name)) !== norm(value)) return false
    try {
      return el.matches(r.sel)
    } catch {
      return false
    }
  })

/** The value the cascade gives `el` for `name`: the rules reaching it, ordered
 * as the cascade orders them (the reader of ListColumns.test.tsx: the
 * specificity counted by `.` and `#`, a tie on source order), the last
 * declaration winning. `styled` says a value reaches the element; this says
 * which value the element ends with, so a global rule of the element's own
 * class (the bar's Delete is a `secondary`) can be told from the scoped rule
 * that beats it. */
const cascadeOf = (el: Element, name: string): string => {
  const hits = reaching(el)
    .map((r, i) => ({
      i,
      spec: (r.sel.match(/[.#]/g) ?? []).length,
      value: prop(r.body, name),
    }))
    .filter((h) => h.value !== '')
    .sort((a, b) => a.spec - b.spec || a.i - b.i)
  return hits.length === 0 ? '' : norm(hits[hits.length - 1].value)
}

// ── The pages ─────────────────────────────────────────────────────────────
const MODEL = '#/profiles/model/Qwen3-27B'
const SERVER = '#/profiles/server/default'
const INSTANCE = '#/instances/coder'

/** One instance the page can remove (16 17c.6a: the record's `removable`, and
 * a state that is not running). */
const REMOVABLE: Instance[] = [
  { id: 'coder', engine: 'llama.cpp', state: 'inactive', removable: true },
]

/** The app at `hash`, loaded to the point the page shows its content. */
async function open(hash: string, options: BffOptions = {}) {
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  return { bff, main }
}

/** The model profile page, of a profile switched off: the page's Delete can
 * act (22 D4.10: `canDelete = !enabled`). */
async function modelPage(off: boolean) {
  const { main } = await open(MODEL, {
    modelProfile: () => reply(200, modelProfileDoc({}, { enabled: !off })),
  })
  // The bar is in the settings section, which the read brings: waiting for the
  // verb is waiting for the page (ModelProfileDelete.test.tsx's `loaded`).
  await within(main).findByRole('button', {
    name: en('model-profile-delete'),
  })
  return main
}

/** The server profile page, of a profile no instance runs: its Delete can act
 * (16 17b: `canDelete = read.instance === null`). */
async function serverPage(inUse: boolean) {
  const { main } = await open(SERVER, {
    serverProfile: () =>
      reply(200, serverProfileDoc({}, { instance: inUse ? 'default' : null })),
  })
  await within(main).findByRole('button', {
    name: en('server-profile-delete'),
  })
  return main
}

/** The Save bar of the page in `main`. */
const barOf = (main: HTMLElement): HTMLElement =>
  main.querySelector('.save-bar')!

/** The buttons of `bar`, in the order the document holds them. */
function barButtons(bar: HTMLElement): HTMLButtonElement[] {
  return Array.from(bar.querySelectorAll('button'))
}

/** The section that holds the page's fields, the one under the heading the page
 *  gives its form. 22 L2.B1 a: the bar is that section's last child. */
function formSection(main: HTMLElement, key: string): HTMLElement {
  const h2 = within(main).getByRole('heading', { level: 2, name: en(key) })
  const section = h2.closest('section')
  expect(
    section,
    `the "${en(key)}" heading stands in the section that holds the fields`,
  ).not.toBeNull()
  return section as HTMLElement
}

/** The controls of `root` the Tab path walks, in the order the document holds
 *  them, with the ones the page keeps out of the path (tabindex="-1": the card
 *  headings the jump list moves the focus to, 2h.196) and the `disabled` ones
 *  out (HearthJumpRail.test.tsx's local reader, a small copy). jsdom lays
 *  nothing out, so a control the sheet hides counts here: no test below reads
 *  the tail of a page that hides a control. */
function stops(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      el.getAttribute('tabindex') !== '-1' &&
      !el.hasAttribute('disabled') &&
      !el.closest('[inert]'),
  )
}

/** The words a keyboard user hears at each of `els`. */
function words(els: HTMLElement[]): string[] {
  return els.map((el) =>
    (el.getAttribute('aria-label') ?? el.textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim(),
  )
}

/** True when `el` is shown as the page stands. jsdom knows the `hidden`
 *  attribute (test/bff.ts's isHidden) and the class the sheet hides with; the
 *  layout it does not know, so a rule that hides by clipping counts too. The
 *  selector is not named: L2.B2 b says the element is visually hidden, not
 *  which way the sheet does it. */
function notShown(el: Element): boolean {
  if (isHidden(el) || el.classList.contains('visually-hidden')) return true
  return RULES.some((r) => {
    const d = norm(prop(r.body, 'display'))
    const v = norm(prop(r.body, 'visibility'))
    const c = norm(prop(r.body, 'clip-path'))
    if (d !== 'none' && v !== 'hidden' && c !== 'inset(50%)') return false
    try {
      return el.matches(r.sel)
    } catch {
      return false
    }
  })
}

/** The element a page Delete describes itself with (22 L2.B2 b: always in the
 *  DOM, with the id `<deleteId>-reason`). */
function reasonOf(main: HTMLElement, del: HTMLElement): HTMLElement {
  const reason = main.querySelector<HTMLElement>(`#${del.id}-reason`)
  expect(
    reason,
    `22 L2.B2 b: ${del.id} has an element of its reason, with the id its description names`,
  ).not.toBeNull()
  return reason!
}

/** The box that owns the pointer and focus handlers of a Delete that cannot act
 *  (22 D4.13, the list rows' shape L2.B2 c asks the page's Delete to keep). */
function boxOf(del: HTMLElement): HTMLElement {
  const box = del.closest('.hint-box')
  expect(
    box,
    'the Delete stands in the box that owns the tip, as a list row’s − does (22 D4.13)',
  ).not.toBeNull()
  return box as HTMLElement
}

/** The tip of `box`, once it is shown. */
async function tipOf(box: HTMLElement): Promise<HTMLElement> {
  await waitFor(() =>
    expect(box.querySelector('.hint-tip'), 'the tip is shown').not.toBeNull(),
  )
  return box.querySelector('.hint-tip') as HTMLElement
}

// ── The rects, stubbed as ListDeleteTip.test.tsx stubs them (22 D4.17): jsdom
// lays nothing out, so a test names the box every control occupies.

type Rect = { top: number; bottom: number; left: number; right: number }

function viewport(width: number, height: number) {
  vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(width)
  vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(height)
}

function rect(el: HTMLElement, r: Rect) {
  el.getBoundingClientRect = () =>
    ({
      ...r,
      x: r.left,
      y: r.top,
      width: r.right - r.left,
      height: r.bottom - r.top,
      toJSON: () => ({}),
    }) as DOMRect
}

function tipSize(el: HTMLElement, width: number, height: number) {
  rect(el, { top: 0, bottom: height, left: 0, right: width })
  Object.defineProperty(el, 'offsetWidth', { value: width, configurable: true })
  Object.defineProperty(el, 'offsetHeight', {
    value: height,
    configurable: true,
  })
}

/** Where the script put the tip, in px, off its inline style. */
function placed(tip: HTMLElement): { top: number; left: number } {
  return {
    top: Number.parseFloat(tip.style.top),
    left: Number.parseFloat(tip.style.left),
  }
}

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  startAt('')
})

// ── Item 1 (L2.B1, the user's ruling of 2026-11-26): the bar at the end of
// the form, sticky at the bottom edge. Its sheet rules are in Reflow.test.ts;
// this is the place and the Tab path L2.B1 a asks for.

test('22 L2.B1 a: on the model profile page the Save bar is the settings section’s last child, after the jump list and every card, and its status line and alert place stand before its buttons', async () => {
  const main = await modelPage(true)
  const bar = barOf(main)
  const section = formSection(main, 'model-profile-settings-title')
  expect(
    bar.parentElement,
    'the bar is a child of the section that holds the fields',
  ).toBe(section)
  expect(
    section.lastElementChild,
    '22 L2.B1 a: the form ends with the bar: sticky bottom holds only an element that comes after the content it floats over, so a bar placed at the head of the section floats over the fields it should sit under',
  ).toBe(bar)
  const nav = within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
  expect(
    before(nav, bar),
    '22 L2.B1 a: the bar comes after the jump list, which 22 D2.5 put after it',
  ).toBe(true)
  const messages = bar.querySelector('.save-messages')
  expect(messages, 'the bar keeps the status line’s place').not.toBeNull()
  expect(
    before(messages!, barButtons(bar)[0]!),
    '22 L2.B1 a: the status line and the alert place stay before the bar’s buttons',
  ).toBe(true)
})

test('22 L2.B1 a: on the model profile page Tab walks the fields first and the bar last: the page’s last three stops are Discard, Save changes and Delete, in that order', async () => {
  const main = await modelPage(true)
  const all = stops(main)
  const tail = all.slice(-3)
  expect(
    words(tail),
    '22 L2.B1 a: the bar is the form’s last element, so its stops are the page’s last, and WCAG 2.4.3 keeps them in the order the reader expects: Discard, Save changes, Delete',
  ).toEqual([en('save-discard'), en('save-button'), en('model-profile-delete')])
  const [discard, save, del] = [tail[0]!, tail[1]!, tail[2]!]
  expect(before(discard, save), 'Discard before Save changes').toBe(true)
  expect(before(save, del), 'Save changes before Delete').toBe(true)
  expect(save.className, 'L2.A2: Save changes is the primary verb').toContain(
    'primary',
  )
  expect(save.className, 'CD.1: and not a danger verb').not.toContain('danger')
})

test('22 L2.A2: the model profile page’s bar Delete is pushed to the bar end and drawn in --danger, and keeps its id, its aria-expanded and a description of its reason (22 D4.10 to D4.14)', async () => {
  const main = await modelPage(false)
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(del.id, 'the verb keeps its id (16 17b.7)').toBe(
    'model-profile-delete',
  )
  expect(del.getAttribute('aria-expanded'), 'the confirm is closed').toBe(
    'false',
  )
  expect(del.getAttribute('aria-disabled'), 'the profile is on').toBe('true')
  expect(
    del.getAttribute('aria-describedby'),
    '22 L2.B2 b: the verb describes itself with its reason',
  ).toBe('model-profile-delete-reason')
  expect(
    styled(del, 'margin-inline-start', 'auto'),
    'the sheet pushes the bar Delete to the end of the row (L2.A2: margin-inline-start: auto)',
  ).toBe(true)
  expect(
    styled(del, 'color', 'var(--danger)'),
    'the sheet draws the bar Delete as a --danger text button (L2.A2)',
  ).toBe(true)
})

test('22 L2.B1 a: on the server profile page the Save bar is the section’s last child, after the last field, and its three stops are the page’s last, reading Discard, Save changes, Delete', async () => {
  const main = await serverPage(false)
  const bar = barOf(main)
  const section = formSection(main, 'server-title')
  expect(
    bar.parentElement,
    'the bar is a child of the section that holds the fields',
  ).toBe(section)
  expect(
    section.lastElementChild,
    '22 L2.B1 a: the server form ends with the bar too',
  ).toBe(bar)
  const tail = stops(main).slice(-3)
  expect(
    words(tail),
    '22 L2.B1 a: its stops are the last of the page, in the order the reader expects',
  ).toEqual([
    en('save-discard'),
    en('save-button'),
    en('server-profile-delete'),
  ])
})

test('22 L2.A2: the server profile page’s bar Delete is pushed to the bar end and drawn in --danger, and keeps its id (16 17b.7)', async () => {
  const main = await serverPage(true)
  const del = within(main).getByRole('button', {
    name: en('server-profile-delete'),
  })
  expect(del.id, 'the verb keeps its id').toBe('server-profile-delete')
  expect(
    styled(del, 'margin-inline-start', 'auto'),
    'the server page Delete is pushed to the bar end too (L2.A2)',
  ).toBe(true)
  expect(styled(del, 'color', 'var(--danger)'), 'and drawn in --danger').toBe(
    true,
  )
})

// ── Item 2 (L2.B2, the user's ruling of 2026-11-26): a Delete that cannot act
// tells why the way a list row’s switched-off trash can does: its reason is an
// element the button describes itself with, never shown, and the words float as
// the tip on hover and on focus. The visible line 22 D4.14 put under Delete is
// gone.

test('22 L2.B2 a, b: a Delete that cannot act stays focusable, is described by an element that is not shown, and a click opens no confirm and sends nothing (model page, profile switched on)', async () => {
  const main = await modelPage(false)
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.2: switched off by aria-disabled, not disabled',
  ).toBe('true')
  expect(
    del.hasAttribute('disabled'),
    '22 L2.B2 a: it keeps the focus (WCAG 4.1.2: a control the keyboard never reaches)',
  ).toBe(false)
  const reason = reasonOf(main, del)
  expect(
    reason.textContent?.replace(/\s+/g, ' ').trim(),
    '22 L2.B2 b: the reason is the key 22 D4.14 gave, unchanged',
  ).toBe(en('model-profiles-delete-needs-off'))
  expect(
    notShown(reason),
    '22 L2.B2 d: the reason is not a visible line of the bar: it is the element the description names, out of the eye, as a list row’s reason is',
  ).toBe(true)
  expect(
    reason.closest('.save-bar'),
    '22 L2.B2 b: it belongs to the bar’s Delete, not to the page',
  ).toBe(barOf(main))
  fireEvent.click(del)
  expect(
    main.querySelector('.confirm-group'),
    '22 L2.B2 a: the click of a Delete that cannot act opens nothing (22 D4.2)',
  ).toBeNull()
})

test('22 L2.B2 c: the model page Delete that cannot act shows its reason as the list’s tip, on hover and on focus, and Escape closes it and keeps the focus on Delete (WCAG 1.4.13)', async () => {
  const main = await modelPage(false)
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  const box = boxOf(del)
  expect(
    del.nextElementSibling,
    '22 D4.13, kept by L2.B2 c: the tip is the button’s next sibling, so the pointer crosses no gap on the way to it',
  ).toBeNull()
  // The hover: the box owns the pointer handlers, and the pointer enters it
  // through the button.
  fireEvent.mouseOver(del)
  const tip = await tipOf(box)
  expect(
    tip.textContent?.replace(/\s+/g, ' ').trim(),
    'the tip says the same words as the description',
  ).toBe(en('model-profiles-delete-needs-off'))
  expect(
    tip.previousElementSibling,
    '22 D4.13: the tip is the Delete’s next sibling inside the box',
  ).toBe(del)
  fireEvent.mouseOut(del, { relatedTarget: document.body })
  await waitFor(() =>
    expect(
      box.querySelector('.hint-tip'),
      'the pointer leaving the box closes the tip',
    ).toBeNull(),
  )
  // The focus: a keyboard user reaches the switched-off verb and hears its
  // description; the words show too.
  del.focus()
  fireEvent.focus(del)
  const byFocus = await tipOf(box)
  expect(
    byFocus.textContent?.replace(/\s+/g, ' ').trim(),
    'the focus shows the same reason',
  ).toBe(en('model-profiles-delete-needs-off'))
  expect(
    document.activeElement,
    'showing the tip moves the focus nowhere',
  ).toBe(del)
  fireEvent.keyDown(del, { key: 'Escape' })
  await waitFor(() =>
    expect(box.querySelector('.hint-tip'), 'Escape closes the tip').toBeNull(),
  )
  expect(
    document.activeElement,
    '22 D4.3: the focus stays on Delete, the user is where they were',
  ).toBe(del)
})

test('22 L2.B2 c: at the viewport’s bottom edge the tip opens above Delete, its right edge at Delete’s right edge, wholly inside a 1280 by 720 viewport (WCAG 2.4.11)', async () => {
  const main = await modelPage(false)
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  const box = boxOf(del)
  viewport(1280, 720)
  // The bar is sticky at the bottom edge, so its Delete stands there: the tip
  // has no room under it.
  const r = { top: 690, bottom: 714, left: 1100, right: 1128 }
  rect(del, r)
  fireEvent.mouseOver(del)
  const tip = await tipOf(box)
  tipSize(tip, 200, 40)
  fireEvent.scroll(window)
  const at = placed(tip)
  expect(
    at,
    '22 L2.B2 c: the bar is at the bottom, so the tip opens above Delete and is right-aligned to Delete’s right edge (the list rows’ placement, at this place in the viewport)',
  ).toEqual({ top: r.top - 40, left: r.right - 200 })
  expect(
    at.top + 40,
    'the tip ends above Delete, so it never covers the verb the pointer is on',
  ).toBeLessThanOrEqual(r.top)
  expect(at.top, 'and lies wholly inside the viewport').toBeGreaterThanOrEqual(
    0,
  )
  expect(at.left + 200, 'with no part past the right edge').toBeLessThanOrEqual(
    1280,
  )
})

test('22 L2.B2: the server profile page’s Delete that cannot act is described by an element that is not shown, and its focus shows the tip (server-profiles-delete-in-use)', async () => {
  const main = await serverPage(true)
  const del = within(main).getByRole('button', {
    name: en('server-profile-delete'),
  })
  expect(
    del.getAttribute('aria-disabled'),
    'an instance runs the profile',
  ).toBe('true')
  const reason = reasonOf(main, del)
  expect(
    reason.textContent?.replace(/\s+/g, ' ').trim(),
    'the key 22 D4.14 named, unchanged',
  ).toBe(en('server-profiles-delete-in-use'))
  expect(
    notShown(reason),
    '22 L2.B2 d: no visible reason line under the server page’s Delete either',
  ).toBe(true)
  del.focus()
  fireEvent.focus(del)
  const tip = await tipOf(boxOf(del))
  expect(
    tip.textContent?.replace(/\s+/g, ' ').trim(),
    'the focus shows the reason, as on the model page',
  ).toBe(en('server-profiles-delete-in-use'))
})

test('22 L2.B2: a Delete that can act names no reason, holds no tip, and opens its confirm (both pages)', async () => {
  const model = await modelPage(true)
  const off = within(model).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    off.getAttribute('aria-describedby'),
    'a verb that can act describes nothing',
  ).toBeNull()
  expect(
    model.querySelector(`#${off.id}-reason`),
    'and there is no reason element for it',
  ).toBeNull()
  fireEvent.mouseOver(off)
  expect(
    boxOf(off).querySelector('.hint-tip'),
    'a hover of a verb that acts shows no tip',
  ).toBeNull()
  fireEvent.click(off)
  expect(
    within(model).getByRole('group', {
      name: en('model-profile-delete-question', 'Qwen3-27B'),
    }),
    'the confirm in place opens as 22 D4.11 has it',
  ).not.toBeNull()
  cleanup()

  const server = await serverPage(false)
  const free = within(server).getByRole('button', {
    name: en('server-profile-delete'),
  })
  expect(
    free.getAttribute('aria-describedby'),
    'the server page’s verb that acts describes nothing',
  ).toBeNull()
  expect(
    server.querySelector(`#${free.id}-reason`),
    'and holds no reason element',
  ).toBeNull()
})
// ── Item 3: CD.1, the four verbs ──────────────────────────────────────────

/** The confirm verb of a page bar, opened by pressing its Delete. */
async function verbOf(main: HTMLElement, opener: string): Promise<HTMLElement> {
  fireEvent.click(within(main).getByRole('button', { name: opener }))
  return within(main).getByRole('button', { name: en('confirm-delete') })
}

/** A list page, read: its table is there, so its rows' − are. */
async function listPage(hash: string, title: string): Promise<HTMLElement> {
  const main = await open(hash).then((r) => r.main)
  await within(main).findByRole('table', { name: title })
  return main
}

/** The page of an instance the record offers for removal (16 17c.6a: the
 * `removable` of the list, and a state that stands still): its opener. */
async function removablePage(): Promise<HTMLElement> {
  const main = await open(INSTANCE, { instances: REMOVABLE }).then(
    (r) => r.main,
  )
  const opener = await within(main).findByRole('button', {
    name: en('instance-remove'),
  })
  fireEvent.click(opener)
  return main
}

test('22 CD.1: the model profile page confirm verb has primary and danger', async () => {
  const main = await modelPage(true)
  const verb = await verbOf(main, en('model-profile-delete'))
  expect(verb.id, 'the verb keeps its id (CD.3)').toBe('confirm-delete')
  expect(verb.className, 'the verb is the filled primary verb').toContain(
    'primary',
  )
  expect(verb.className, 'and CD.1 gives it danger').toContain('danger')
})

test('22 CD.1: the server profile page confirm verb has primary and danger', async () => {
  const main = await serverPage(false)
  const verb = await verbOf(main, en('server-profile-delete'))
  expect(verb.id, 'the verb keeps its id').toBe('confirm-delete')
  expect(verb.className).toContain('primary')
  expect(verb.className, 'CD.1: danger on the verb that destroys').toContain(
    'danger',
  )
})

test('22 CD.1: the Models list row confirm verb has primary and danger', async () => {
  const main = await listPage('#/profiles/model', en('model-profiles-title'))
  // The row of the switched-off profile: its − acts (22 D4.2).
  const minus = within(main).getByRole('button', {
    name: en('model-profiles-delete-row', 'gemma-3-12b-it'),
  })
  fireEvent.click(minus)
  const verb = within(main).getByRole('button', { name: en('confirm-delete') })
  expect(verb.id, 'the verb keeps its id').toBe('confirm-delete')
  expect(verb.className).toContain('primary')
  expect(verb.className, 'CD.1: danger on the list verb too').toContain(
    'danger',
  )
})

test('22 CD.1: the Server profiles list row confirm verb has primary and danger', async () => {
  const main = await listPage('#/profiles/server', en('server-profiles-title'))
  const minus = within(main).getByRole('button', {
    name: en('server-profiles-delete-row', 'default'),
  })
  fireEvent.click(minus)
  const verb = within(main).getByRole('button', { name: en('confirm-delete') })
  expect(verb.id, 'the verb keeps its id').toBe('confirm-delete')
  expect(verb.className).toContain('primary')
  expect(verb.className, 'CD.1: danger on the Server list verb too').toContain(
    'danger',
  )
})

test('22 CD.1: the instance page remove verb has primary and danger', async () => {
  const main = await removablePage()
  const verb = within(main).getByRole('button', { name: en('confirm-remove') })
  expect(verb.id, 'the verb keeps its id').toBe('confirm-remove')
  expect(verb.className).toContain('primary')
  expect(verb.className, 'CD.1: danger on the remove verb').toContain('danger')
})

test('22 CD.1: Save changes, the two Create verbs and the display name Save keep primary without danger', async () => {
  const main = await modelPage(true)
  const save = within(main).getByRole('button', { name: en('save-button') })
  expect(save.className).toContain('primary')
  expect(save.className, 'the save is not a danger verb').not.toContain(
    'danger',
  )
  // The create forms of both lists, and the display name form.
  cleanup()
  const models = await listPage('#/profiles/model', en('model-profiles-title'))
  fireEvent.click(
    within(models).getByRole('button', { name: en('model-profiles-new') }),
  )
  const created = await within(models).findByRole('button', {
    name: en('create-button'),
  })
  expect(
    created.className,
    'the Models create form keeps its plain primary verb',
  ).toContain('primary')
  expect(created.className).not.toContain('danger')
  cleanup()
  const servers = await open('#/profiles/server').then((r) => r.main)
  // The pool read answers before the list shows its add.
  await within(servers).findByRole('table', {
    name: en('server-profiles-title'),
  })
  fireEvent.click(
    within(servers).getByRole('button', { name: en('server-profiles-new') }),
  )
  const made = await within(servers).findByRole('button', {
    name: en('create-button'),
  })
  expect(
    made.className,
    'the Server create form keeps its plain primary verb',
  ).toContain('primary')
  expect(made.className).not.toContain('danger')
  cleanup()
  const page = await open(INSTANCE).then((r) => r.main)
  // The pencil is in the title row the read fills: finding it is waiting for
  // the page.
  const pencil = await within(page).findByRole('button', {
    name: en('instance-name-edit'),
  })
  fireEvent.click(pencil)
  const named = await within(page).findByRole('button', {
    name: en('instance-name-save'),
  })
  expect(
    named.className,
    'the display name Save keeps its plain primary verb',
  ).toContain('primary')
  expect(named.className).not.toContain('danger')
})

// ── Item 5: CD.3, unchanged ───────────────────────────────────────────────

/** The confirm's two verbs: text, order, and where the focus is. */
function checkConfirm(main: HTMLElement, verbName: string) {
  const verb = within(main).getByRole('button', { name: verbName })
  const cancel = within(main).getByRole('button', {
    name: en('confirm-cancel'),
  })
  expect(verb.textContent, 'the verb says what it always said').toBe(verbName)
  expect(
    before(verb, cancel),
    `the verb comes first, then Cancel (${verbName})`,
  ).toBe(true)
  expect(
    document.activeElement,
    `the focus is on Cancel, the verb one Tab away (${verbName})`,
  ).toBe(cancel)
}

test('22 CD.3: the four confirms keep their texts, their order of verb then Cancel, and their focus on Cancel', async () => {
  const model = await modelPage(true)
  fireEvent.click(
    within(model).getByRole('button', { name: en('model-profile-delete') }),
  )
  checkConfirm(model, en('confirm-delete'))
  cleanup()

  const server = await serverPage(false)
  fireEvent.click(
    within(server).getByRole('button', { name: en('server-profile-delete') }),
  )
  checkConfirm(server, en('confirm-delete'))
  cleanup()

  const models = await listPage('#/profiles/model', en('model-profiles-title'))
  fireEvent.click(
    within(models).getByRole('button', {
      name: en('model-profiles-delete-row', 'gemma-3-12b-it'),
    }),
  )
  checkConfirm(models, en('confirm-delete'))
  cleanup()

  const page = await removablePage()
  checkConfirm(page, en('confirm-remove'))
})

// ── The design-review round of 2026-11-27: D4.30's class, and its second
//    review's D4, which withdraws advice 1 (the short verb) ─────────────────

test('22 L2 D4.30 (the code review’s class item): the page Delete carries a class of its own, and the sheet keys the end-of-row margin and the danger colour on that class, not on the last button of the bar', async () => {
  const main = await modelPage(true)
  const bar = barOf(main)
  const del = within(bar).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    del.className.split(/\s+/),
    'the Delete of the Save bar is named by a class of its own (the reviewer’s example, `save-bar-delete`)',
  ).toContain('save-bar-delete')
  // The two declarations reach it.
  for (const [name, value] of [
    ['margin-inline-start', 'auto'],
    ['color', 'var(--danger)'],
  ])
    expect(
      styled(del, name, value),
      `the sheet gives the page Delete ${name}: ${value}`,
    ).toBe(true)
  // And reaches it by that class alone: a rule keyed on the position in the
  // group (the `:last-of-type` of the rule today) makes a second verb appear
  // outside the class the moment the bar grows one.
  expect(
    reaching(del)
      .filter((r) => r.sel.includes(':last-of-type'))
      .map((r) => r.sel),
    'no rule of the bar reaches the page Delete by its place in the group',
  ).toEqual([])
  // The end-of-row margin is the class's alone: the bar has no rule of its own
  // giving any verb a margin-inline-start, so a positional rule (the
  // `:last-of-type` of the rule today) is what this reds.
  expect(
    declaring(del, 'margin-inline-start').filter(
      (rule) => !rule.includes('.save-bar-delete'),
    ),
    'every rule that gives the page Delete its end-of-row margin keys on its class: ' +
      declaring(del, 'margin-inline-start').join(' | '),
  ).toEqual([])
  // The colour is not the same case: the sheet's global
  // `.secondary { color: var(--text) }` reaches the Delete, as it reaches every
  // secondary button, and the review does not ask it gone. What it asks is that
  // the destructive colour comes from the class and beats it, so the cascade
  // answers --danger for a Delete that can act, and every rule that says
  // --danger to the Delete names the class (a rule keyed on the place in the
  // group would colour any verb added after it, outside the class).
  expect(
    cascadeOf(del, 'color'),
    'the class rule beats the global .secondary colour, so the page Delete reads --danger',
  ).toBe('var(--danger)')
  expect(
    cascadeOf(del, 'margin-inline-start'),
    'and the cascade pushes it to the end of the bar',
  ).toBe('auto')
  expect(
    declaring(del, 'color')
      .filter(
        (rule) =>
          rule.includes('var(--danger)') && !rule.includes('.save-bar-delete'),
      )
      .map((rule) => rule.split(':')[0]),
    'every rule giving the page Delete the danger colour keys on its class',
  ).toEqual([])
})

test('22 L2 D4.30 review A1, its advice 1 withdrawn: the page Delete has ONE visible label, the full verb, on the model page and on the server page (the second review, D4)', async () => {
  const cases = [
    { open: () => modelPage(true), key: 'model-profile-delete' },
    { open: () => serverPage(false), key: 'server-profile-delete' },
  ]
  for (const c of cases) {
    const main = await c.open()
    const del = within(barOf(main)).getByRole('button', {
      name: en(c.key),
    })
    const text = norm(del.textContent ?? '')
    expect(
      text,
      `${en(c.key)}: the button's own text is the full verb, the one label the eye reads (advice 1's short verb is withdrawn: at 320 px a short Delete buys no row, only a less clear button)`,
    ).toBe(en(c.key))
    // The review's test: the visible text is the accessible name. Read through
    // test/bff.ts's accName, which answers the `aria-label` when there is one.
    expect(
      accName(del),
      `${en(c.key)}: a reader says what the eye reads (WCAG 2.5.3)`,
    ).toBe(text)
    expect(
      del.hasAttribute('aria-label'),
      `${en(c.key)}: no aria-label duplicating the text: the text names the button`,
    ).toBe(false)
    expect(
      del.children.length,
      `${en(c.key)}: the verb is the button's own text, with no span of a short and a long run inside`,
    ).toBe(0)
    expect(
      del.className
        .split(/\s+/)
        .filter((cls) => cls.startsWith('save-bar-delete-')),
      `${en(c.key)}: the button carries no label class of the two spans`,
    ).toEqual([])
    // Nothing of the sheet hides a label of the bar's Delete at any width. The
    // defect: the narrow block's `display: inline` on the short span sitting
    // before the global `display: none` of the same class, so at 320 px both
    // spans were hidden and the button was a 34 px oval of nothing. jsdom
    // applies no media query, so this reads the whole sheet, the narrow blocks
    // included.
    expect(
      reaching(del)
        .filter((r) => norm(prop(r.body, 'display')) === 'none')
        .map((r) => `${r.sel}: display: none`),
      `${en(c.key)}: no rule of the sheet, the narrow blocks included, hides the verb`,
    ).toEqual([])
    cleanup()
  }
  // The two spans' rules leave the sheet: a rule naming one of their classes is
  // the shape that broke (index.css 727-734 and 2737-2739, the pair of the same
  // class with the same specificity, the later winning).
  expect(
    RULES.filter((r) => /save-bar-delete-(short|long)/.test(r.sel)).map(
      (r) => `${r.sel}: ${norm(r.body).slice(0, 40)}`,
    ),
    'no rule of the sheet names the short or the long label class',
  ).toEqual([])
})
