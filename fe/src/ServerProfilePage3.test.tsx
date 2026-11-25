// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4): the
// server profile page's Delete in the Save bar, the parts the amended 17b tests
// do not pin: the disabled state and the reason it shows (22 D4.10, with
// 22 D4.14: the reason is a line of the bar, shown all the time while the
// Delete cannot act, never a tip on hover), and that the Delete's state does
// not follow the form's dirty state. The place of the opener and of the group,
// the bar's order, the confirm's answers and the refusals are
// ServerProfilePage2.test.tsx as 22 D4 amends them; the list's − is
// ServerProfilesDelete1.test.tsx. The model page's twin is
// ModelProfileDelete3.test.tsx.
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
  fakeBff,
  isHidden,
  reply,
  serverProfileDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'qwen-27b'
const HASH = `#/profiles/server/${ID}`
const ROUTE = `/api/profiles/server/${ID}`
/** The instance that uses the profile. */
const TAKEN = 'coder'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(async () => {
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a tip opening, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The document of a profile no instance runs. */
function freeDoc() {
  return reply(200, serverProfileDoc({}, { instance: null }))
}

/** The document of a profile instance `instance` runs. */
function usedDoc(instance: string) {
  return reply(200, serverProfileDoc({}, { instance }))
}

async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ serverProfile: () => freeDoc(), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('server-profile-delete') })
}

/** The Server form's Port box: the page's own field, used to make the page
 *  hold an edit. */
function portBox(main: HTMLElement): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: en('server-port') })
  return (
    g
      ? within(g).getByRole('textbox', { name: en('server-port') })
      : within(main).getByRole('textbox', { name: en('server-port') })
  ) as HTMLInputElement
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

// ── The stylesheet reader (22 D4.15 pins, the local copy) ─────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = { compact: string; body: string; at: number; depth: number }

const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

const TOP = RULES.filter((r) => r.depth === 0)

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

// ── 22 D4.10, D4.14: the disabled Delete and its reason ─────────────────

test('22 D4.10, D4.14: a used profile’s bar Delete is aria-disabled and focusable, and while it cannot act a p#server-profile-delete-reason with server-profiles-delete-in-use stands in the bar after its buttons, shown all the time and named by Delete’s aria-describedby; no .hint-tip shows at rest, on hover or on focus; a press opens nothing and sends nothing, and the p is gone when Delete can act', async () => {
  const { bff, main } = await loaded({ serverProfile: () => usedDoc(TAKEN) })
  const del = deleteButton(main)
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.10: an instance uses it, so the Delete cannot act',
  ).toBe('true')
  expect(
    (del as HTMLButtonElement).disabled,
    '22 D4.2: aria-disabled, not disabled: it stays focusable',
  ).toBe(false)
  // 22 D4.14: the reason is a line of the bar, there without asking, and it is
  // what the Delete names.
  const describedBy = del.getAttribute('aria-describedby')
  expect(describedBy, '22 D4.14: the Delete names its reason').toBe(
    'server-profile-delete-reason',
  )
  const reason = main.querySelector('#server-profile-delete-reason')
  expect(reason, 'the reason is on the page, unasked').not.toBeNull()
  expect(reason, 'and it is the element the Delete names').toBe(
    document.getElementById(describedBy!),
  )
  expect(reason!.tagName.toLowerCase(), '22 D4.14: a p').toBe('p')
  expect(text(reason), '22 D4.14: the list’s reason, shared').toBe(
    en('server-profiles-delete-in-use'),
  )
  expect(reason!.closest('button'), 'not inside a button').toBeNull()
  expect(
    reason!.classList.contains('visually-hidden') || isHidden(reason),
    '22 D4.14: shown to the eye, not only read',
  ).toBe(false)
  // It is not content on hover: it stands at rest, through a hover, a focus, a
  // blur and the pointer leaving, and no .hint-tip is ever there.
  const said = () => text(main.querySelector('#server-profile-delete-reason'))
  expect(main.querySelector('.hint-tip'), '22 D4.14: no tip at rest').toBeNull()
  fireEvent.mouseOver(del)
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    '22 D4.14: no tip on hover',
  ).toBeNull()
  expect(said(), 'the reason stands').toBe(en('server-profiles-delete-in-use'))
  del.focus()
  await flushTasks()
  expect(main.querySelector('.hint-tip'), 'no tip on focus').toBeNull()
  expect(
    said(),
    '22 D4.14: the focus leaves the reason alone: it is not hover content',
  ).toBe(en('server-profiles-delete-in-use'))
  del.blur()
  fireEvent.mouseOut(del, { relatedTarget: document.body })
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    'no tip once the pointer and the focus are gone',
  ).toBeNull()
  expect(said(), 'and the reason is still in the bar').toBe(
    en('server-profiles-delete-in-use'),
  )
  // A press of the disabled Delete sends nothing and opens nothing.
  press(del)
  expect(
    main.querySelector('.save-bar [role="group"]'),
    'a press opens no confirm',
  ).toBeNull()
  expect(bff.to('DELETE', ROUTE), 'and sends nothing').toHaveLength(0)
  cleanup()
  vi.unstubAllGlobals()
  // A profile that can be deleted: no reason line, no description, no tip.
  const free = await loaded()
  const active = deleteButton(free.main)
  expect(
    active.getAttribute('aria-disabled'),
    'a free profile: the Delete can act',
  ).not.toBe('true')
  expect(
    active.getAttribute('aria-describedby'),
    '22 D4.14: it names no reason',
  ).toBeNull()
  expect(
    free.main.querySelector('#server-profile-delete-reason'),
    '22 D4.14: the p is gone when the Delete can act',
  ).toBeNull()
  fireEvent.mouseOver(active)
  active.focus()
  await flushTasks()
  expect(
    free.main.querySelector('.hint-tip'),
    '22 D4.3: an active Delete shows no tip',
  ).toBeNull()
})

// ── 22 D4.14: the reason is a line of the bar (22D4 review D1) ──────────

test('22 D4.14 (22 D4 review D1): the reason is a line of the Save bar, a child of the bar after its buttons where the confirm opens, not a box wrapped around the Delete, and the sheet gives .hint-tip no position: absolute or fixed', async () => {
  const { main } = await loaded({ serverProfile: () => usedDoc(TAKEN) })
  const del = deleteButton(main)
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').not.toBeNull()
  const actions = bar!.querySelector('.save-actions')
  expect(actions, 'the bar’s buttons').not.toBeNull()
  const reason = main.querySelector('#server-profile-delete-reason')
  expect(reason, 'the reason is there, unasked').not.toBeNull()
  expect(reason!.closest('.save-bar'), 'in the bar').toBe(bar)
  expect(
    reason!.parentElement,
    '22 D4.14: a line of the bar, not a box around the Delete',
  ).toBe(bar)
  expect(
    before(actions!, reason!),
    '22 D4.14: after the buttons, where the confirm opens',
  ).toBe(true)
  expect(before(del, reason!), 'and after the Delete').toBe(true)
  // 22 D4.15 as far as jsdom reaches, the sheet text: a `position: absolute`
  // or `fixed` takes the box out of the flow, which is what the decision
  // removed, so no rule that reaches the reason declares one.
  const decls = TOP.filter((r) =>
    r.compact.split(',').includes('.hint-tip'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(decls.length, 'the sheet has a .hint-tip rule').toBeGreaterThan(0)
  expect(
    decls.filter((d) => /^position:(absolute|fixed)$/.test(d)),
    '22 D4.15: no absolute, no fixed, nothing on a layer of its own',
  ).toEqual([])
})

// ── 22 D4.10: the state does not follow dirty ────────────────────────────

test('22 D4.10: the Delete’s state does not follow the form’s dirty state: an edit leaves the active Delete active on a free profile, and the disabled one disabled on a used one', async () => {
  const { main } = await loaded()
  expect(
    deleteButton(main).getAttribute('aria-disabled'),
    'active at rest',
  ).not.toBe('true')
  fireEvent.change(portBox(main), { target: { value: '9999' } })
  expect(
    deleteButton(main).getAttribute('aria-disabled'),
    '22 D4.10: still active with the page holding an edit',
  ).not.toBe('true')
  cleanup()
  vi.unstubAllGlobals()
  const used = await loaded({ serverProfile: () => usedDoc(TAKEN) })
  expect(
    deleteButton(used.main).getAttribute('aria-disabled'),
    'disabled at rest',
  ).toBe('true')
  fireEvent.change(portBox(used.main), { target: { value: '9999' } })
  expect(
    deleteButton(used.main).getAttribute('aria-disabled'),
    '22 D4.10: still disabled with the page holding an edit',
  ).toBe('true')
})
