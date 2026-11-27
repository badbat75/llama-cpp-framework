// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4), as slice
// L2's 22 L2.B2 amends it (fe/briefs/22-model-sources.md, the L2 acceptance
// list): the server profile page's Delete in the Save bar, the parts the
// amended 17b tests do not pin: the disabled state and the reason it shows
// (22 D4.10; 22 D4.14 read a line of the bar, shown all the time, and L2.B2
// replaced that: the reason is a visually hidden element the Delete describes
// itself with, and the words float as the list rows' tip on hover and on
// focus), and that the Delete's state does not follow the form's dirty state.
// The place of the opener and of the group, the bar's order, the confirm's
// answers and the refusals are ServerProfilePage2.test.tsx as 22 D4 amends
// them; the list's − is ServerProfilesDelete1.test.tsx. The model page's twin
// is ModelProfileDelete3.test.tsx.
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
import {
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

/** True when `el` is not shown as the page stands: the hidden attribute and
 *  the class the sheet hides with (test/bff.ts's isHidden, the list rows'
 *  visually hidden reason), or a declaration of a top-level sheet rule that
 *  reaches the element taking it out of the eye (22 L2.B2 b; jsdom lays
 *  nothing out, so the sheet text stands in for the layout). */
function notShown(el: Element): boolean {
  if (isHidden(el) || el.classList.contains('visually-hidden')) return true
  return reaches(el).some(
    (d) =>
      /^display:\s*none$/.test(d) ||
      /^visibility:\s*hidden$/.test(d) ||
      /^clip-path:\s*inset\(\s*50%\s*\)$/.test(d),
  )
}

/** The page's shown tip, once it is there (the tip opens on a queued task, as
 *  in ListDeleteTip.test.tsx). */
async function shownTip(main: HTMLElement): Promise<HTMLElement> {
  const tip = await waitFor(() => {
    const found = main.querySelector('.hint-tip')
    expect(found, 'the tip is shown').not.toBeNull()
    return found
  })
  return tip as HTMLElement
}

// ── The stylesheet reader (22 L2.B2 pins, the local copy) ─────────────

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

/** The declarations of every top-level rule that reaches `el`, in the sheet's
 *  order: a selector part whose last token is the element's tag, class or id,
 *  and whose earlier tokens are among its ancestors' (the cascade reader of
 *  22 D4.16, read for the bar's classless `p`). A pseudo-class on the element
 *  (`p:hover`) is not a rule that places it, so this matches none. */
function reaches(el: Element): string[] {
  const own = new Set([el.tagName.toLowerCase()])
  for (const c of el.classList) own.add(`.${c}`)
  if (el.id) own.add(`#${el.id}`)
  const anc = new Set<string>()
  for (let n = el.parentElement; n; n = n.parentElement) {
    anc.add(n.tagName.toLowerCase())
    for (const c of n.classList) anc.add(`.${c}`)
    if (n.id) anc.add(`#${n.id}`)
  }
  return TOP.filter((r) =>
    r.compact.split(',').some((part) => {
      const tokens = part.match(/[#.]?[-a-z0-9_]+/g) ?? []
      if (tokens.length === 0) return false
      return (
        own.has(tokens[tokens.length - 1]) &&
        tokens.slice(0, -1).every((t) => anc.has(t))
      )
    }),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
}

// ── 22 D4.10 with 22 L2.B2: the disabled Delete and its reason ───────────

test('22 D4.10, D4.14 as 22 L2.B2 amends it: a used profile’s bar Delete is aria-disabled and focusable, describes itself with a not-shown #server-profile-delete-reason element, and the words show as the list’s .hint-tip on hover and on focus; Escape closes the tip and leaves the focus on Delete; a press opens nothing and sends nothing, and a Delete that can act names no reason and shows no tip', async () => {
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
  // 22 L2.B2 b: the Delete describes itself with its reason, and the reason is
  // an element of the page, not a line the eye reads.
  const describedBy = del.getAttribute('aria-describedby')
  expect(describedBy, '22 L2.B2 b: the Delete names its reason').toBe(
    'server-profile-delete-reason',
  )
  const reason = main.querySelector('#server-profile-delete-reason')
  expect(reason, 'the reason is on the page, unasked').not.toBeNull()
  expect(reason, 'and it is the element the Delete names').toBe(
    document.getElementById(describedBy!),
  )
  expect(text(reason), '22 D4.14: the list’s reason, shared').toBe(
    en('server-profiles-delete-in-use'),
  )
  expect(reason!.closest('button'), 'not inside a button').toBeNull()
  expect(
    notShown(reason!),
    '22 L2.B2 b: the reason is visually hidden: no line of the bar, and 22 D4.14’s shown p is gone',
  ).toBe(true)
  // The words float: the list rows’ tip, on hover and on focus (22 L2.B2 c).
  expect(main.querySelector('.hint-tip'), 'no tip at rest').toBeNull()
  fireEvent.mouseOver(del)
  const tip = await shownTip(main)
  expect(text(tip), 'the hover shows the reason').toBe(
    en('server-profiles-delete-in-use'),
  )
  fireEvent.mouseOut(del, { relatedTarget: document.body })
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    'the pointer leaving the box closes the tip',
  ).toBeNull()
  del.focus()
  fireEvent.focus(del)
  const byFocus = await shownTip(main)
  expect(text(byFocus), 'the focus shows the same reason').toBe(
    en('server-profiles-delete-in-use'),
  )
  expect(
    document.activeElement,
    'showing the tip moves the focus nowhere',
  ).toBe(del)
  fireEvent.keyDown(document, { key: 'Escape' })
  await flushTasks()
  expect(
    main.querySelector('.hint-tip'),
    '22 D4.18 D3: Escape closes the tip',
  ).toBeNull()
  expect(
    document.activeElement,
    '22 D4.3: the focus stays on Delete: the user is where they were',
  ).toBe(del)
  // A press of the disabled Delete sends nothing and opens nothing.
  press(del)
  expect(
    main.querySelector('.save-bar [role="group"]'),
    'a press opens no confirm',
  ).toBeNull()
  expect(bff.to('DELETE', ROUTE), 'and sends nothing').toHaveLength(0)
  cleanup()
  vi.unstubAllGlobals()
  // A profile that can be deleted: no reason, no description, no tip.
  const free = await loaded()
  const active = deleteButton(free.main)
  expect(
    active.getAttribute('aria-disabled'),
    'a free profile: the Delete can act',
  ).not.toBe('true')
  expect(
    active.getAttribute('aria-describedby'),
    '22 L2.B2: a verb that can act names no reason',
  ).toBeNull()
  expect(
    free.main.querySelector('#server-profile-delete-reason'),
    '22 L2.B2: the reason element is gone with the description',
  ).toBeNull()
  fireEvent.mouseOver(active)
  active.focus()
  await flushTasks()
  expect(
    free.main.querySelector('.hint-tip'),
    '22 D4.3: an active Delete shows no tip',
  ).toBeNull()
})

// ── 22 L2.B2: the reason floats, it is not a line of the bar ─────────────

test('22 L2.B2 c (replacing 22 D4.14’s line, 22 D4 review D1): the reason of the bar’s Delete that cannot act floats over the page: the tip is the Delete’s next sibling in the box that owns the hover and the focus, the box stands in the bar, and the sheet takes the tip out of the flow, so no reason adds a line to the bar and the bar’s height never changes', async () => {
  const { main } = await loaded({ serverProfile: () => usedDoc(TAKEN) })
  const del = deleteButton(main)
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').not.toBeNull()
  const reason = main.querySelector('#server-profile-delete-reason')
  expect(reason, 'the reason is there, unasked').not.toBeNull()
  expect(
    reason!.closest('.save-bar'),
    '22 L2.B2 b: it belongs to the bar’s Delete, not to the page',
  ).toBe(bar)
  const box = del.closest('.hint-box')
  expect(
    box,
    '22 L2.B2 c: the Delete stands in the box that owns the tip, as a list row’s − does (22 D4.13)',
  ).not.toBeNull()
  expect(
    box!.closest('.save-bar'),
    'the box stands in the bar: the Delete did not leave it',
  ).toBe(bar)
  fireEvent.mouseOver(del)
  const tip = await shownTip(main)
  expect(tip.parentElement, 'the tip is a child of the box').toBe(box)
  expect(
    tip.previousElementSibling,
    '22 D4.13, kept by 22 L2.B2 c: the tip is the Delete’s next sibling, so the pointer crosses no gap on the way to it',
  ).toBe(del)
  // The sheet text, as far as jsdom reaches: the tip is out of the flow, so
  // showing it changes no size in the bar (22 L2.B2 c: "changes no size in the
  // bar"). Reflow.test.ts pins the .hint-tip rule itself.
  const decls = reaches(tip)
  expect(
    decls.filter((d) => /^position:\s*(absolute|fixed)$/.test(d)).length,
    'the tip takes no room in the bar: it is out of the flow',
  ).toBeGreaterThan(0)
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
