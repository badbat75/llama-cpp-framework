// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4): the model
// profile page's Delete in the Save bar, the parts the amended 22D and 22D2
// tests do not pin: the disabled state and the reason it shows (22 D4.10, with
// 22 D4.14: the reason is a line of the bar, shown all the time while the
// Delete cannot act, never a tip on hover), and that the Delete's state does
// not follow the form's dirty state. The place of the opener and of the group,
// the bar's order, the gap, the confirm's answers and the refusals are
// ModelProfileDelete.test.tsx and ModelProfileDelete2.test.tsx as 22 D4 amends
// them; the list's − is ModelProfilesDelete1.test.tsx.
//
// Not provable in jsdom, so not tested here: the reason's look at 320 px and
// 200 % text, which the UX check measures (fe/ux/check.js, edited by another
// agent).
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
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`

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

/** The document of a switched-off profile, with the two Embeddings keys set so
 *  its select is editable (the page can hold an edit). */
function offDoc() {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id: ID, enabled: false },
    ),
  )
}

/** The document of a switched-on profile. */
function onDoc() {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id: ID, enabled: true },
    ),
  )
}

async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfile: () => offDoc(), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  // The read lands with the cards: the h1 and the loading line are there
  // before it, and the bar's Delete is only in the page after it.
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('model-profile-delete') })
}

/** Changes the Embeddings card's mode select: the page holds an edit. */
function makeDirty(main: HTMLElement) {
  const select = within(main).getByRole('combobox', {
    name: en('model-profile-embedding-mode'),
  })
  fireEvent.change(select, { target: { value: 'false' } })
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

test('22 D4.10, D4.14: a switched-on profile’s bar Delete is aria-disabled and focusable, and while it cannot act a p#model-profile-delete-reason with model-profiles-delete-needs-off stands in the bar after its buttons, shown all the time and named by Delete’s aria-describedby; no .hint-tip shows at rest, on hover or on focus; a press opens nothing and sends nothing, and the p is gone when Delete can act', async () => {
  const { bff, main } = await loaded({ modelProfile: () => onDoc() })
  const del = deleteButton(main)
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.10: switched on, so the Delete cannot act',
  ).toBe('true')
  expect(
    (del as HTMLButtonElement).disabled,
    '22 D4.2: aria-disabled, not disabled: it stays focusable',
  ).toBe(false)
  // 22 D4.14: the reason is a line of the bar, there without asking, and it is
  // what the Delete names.
  const describedBy = del.getAttribute('aria-describedby')
  expect(describedBy, '22 D4.14: the Delete names its reason').toBe(
    'model-profile-delete-reason',
  )
  const reason = main.querySelector('#model-profile-delete-reason')
  expect(reason, 'the reason is on the page, unasked').not.toBeNull()
  expect(reason, 'and it is the element the Delete names').toBe(
    document.getElementById(describedBy!),
  )
  expect(reason!.tagName.toLowerCase(), '22 D4.14: a p').toBe('p')
  expect(text(reason), '22 D4.14: the list’s reason, shared').toBe(
    en('model-profiles-delete-needs-off'),
  )
  expect(reason!.closest('button'), 'not inside a button').toBeNull()
  expect(
    reason!.classList.contains('visually-hidden') || isHidden(reason),
    '22 D4.14: shown to the eye, not only read',
  ).toBe(false)
  // It is not content on hover: it stands at rest, through a hover, a focus, a
  // blur and the pointer leaving, and no .hint-tip is ever there.
  const said = () => text(main.querySelector('#model-profile-delete-reason'))
  expect(main.querySelector('.hint-tip'), '22 D4.14: no tip at rest').toBeNull()
  fireEvent.mouseOver(del)
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    '22 D4.14: no tip on hover',
  ).toBeNull()
  expect(said(), 'the reason stands').toBe(
    en('model-profiles-delete-needs-off'),
  )
  del.focus()
  await flushTasks()
  expect(main.querySelector('.hint-tip'), 'no tip on focus').toBeNull()
  expect(
    said(),
    '22 D4.14: the focus leaves the reason alone: it is not hover content',
  ).toBe(en('model-profiles-delete-needs-off'))
  del.blur()
  fireEvent.mouseOut(del, { relatedTarget: document.body })
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    'no tip once the pointer and the focus are gone',
  ).toBeNull()
  expect(said(), 'and the reason is still in the bar').toBe(
    en('model-profiles-delete-needs-off'),
  )
  // A press of the disabled Delete sends nothing and opens nothing.
  press(del)
  expect(
    main.querySelector('.save-bar [role="group"]'),
    'a press opens no confirm',
  ).toBeNull()
  expect(
    bff.to('DELETE', `/api/profiles/model/${ID}`),
    'and sends nothing',
  ).toHaveLength(0)
  cleanup()
  vi.unstubAllGlobals()
  // A profile that can be deleted: no reason line, no description, no tip.
  const off = await loaded()
  const active = deleteButton(off.main)
  expect(
    active.getAttribute('aria-disabled'),
    'a switched-off profile: the Delete can act',
  ).not.toBe('true')
  expect(
    active.getAttribute('aria-describedby'),
    '22 D4.14: it names no reason',
  ).toBeNull()
  expect(
    off.main.querySelector('#model-profile-delete-reason'),
    '22 D4.14: the p is gone when the Delete can act',
  ).toBeNull()
  fireEvent.mouseOver(active)
  active.focus()
  await flushTasks()
  expect(
    off.main.querySelector('.hint-tip'),
    '22 D4.3: an active Delete shows no tip',
  ).toBeNull()
})

// ── 22 D4.14: the reason is a line of the bar (22D4 review D1) ──────────

test('22 D4.14 (22 D4 review D1): the reason is a line of the Save bar, a child of the bar after its buttons where the confirm opens, not a box wrapped around the Delete, and the sheet gives .hint-tip no position: absolute or fixed', async () => {
  const { main } = await loaded({ modelProfile: () => onDoc() })
  const del = deleteButton(main)
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').not.toBeNull()
  const actions = bar!.querySelector('.save-actions')
  expect(actions, 'the bar’s buttons').not.toBeNull()
  const reason = main.querySelector('#model-profile-delete-reason')
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

test('22 D4.10: the Delete’s state does not follow the form’s dirty state: an edit leaves the active Delete active on a switched-off profile, and the disabled one disabled on a switched-on one', async () => {
  const { main } = await loaded()
  const before = deleteButton(main)
  expect(before.getAttribute('aria-disabled'), 'active at rest').not.toBe(
    'true',
  )
  makeDirty(main)
  const after = deleteButton(main)
  expect(
    after.getAttribute('aria-disabled'),
    '22 D4.10: still active with the page holding an edit',
  ).not.toBe('true')
  cleanup()
  vi.unstubAllGlobals()
  const on = await loaded({ modelProfile: () => onDoc() })
  expect(
    deleteButton(on.main).getAttribute('aria-disabled'),
    'disabled at rest',
  ).toBe('true')
  makeDirty(on.main)
  expect(
    deleteButton(on.main).getAttribute('aria-disabled'),
    '22 D4.10: still disabled with the page holding an edit',
  ).toBe('true')
})
