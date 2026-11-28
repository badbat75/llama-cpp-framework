// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4), as slice
// L2's 22 L2.B2 amends it (fe/briefs/22-model-sources.md, the L2 acceptance
// list): the model profile page's Delete in the Save bar, the parts the
// amended 22D and 22D2 tests do not pin: the disabled state and the reason it
// shows (22 D4.10; 22 D4.14 read a line of the bar, shown all the time, and
// L2.B2 replaced that: the reason is a visually hidden element the Delete
// describes itself with, and the words float as the list rows' tip on hover
// and on focus), and that the Delete's state does not follow the form's dirty
// state. The place of the opener and of the group, the bar's order, the gap,
// the confirm's answers and the refusals are ModelProfileDelete.test.tsx and
// ModelProfileDelete2.test.tsx as 22 D4 amends them; the list's − is
// ModelProfilesDelete1.test.tsx; L2's bar-wide pins are L2SaveBar.test.tsx.
//
// Not provable in jsdom, so not tested here: the tip's place at the viewport's
// bottom edge and its look at 320 px and 200 % text, which the designer
// measures (fe/ux/check.js, edited by another agent).
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

/** The document of a profile no instance profile lists, so the page's Delete
 *  can act (22 IP5.3: the assignment is the one reason it cannot). The two
 *  Embeddings keys are set so its select is editable: the page can hold an
 *  edit. */
function freeDoc() {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id: ID, usedBy: [] },
    ),
  )
}

/** The document of a profile an instance profile lists: the Delete cannot act. */
function usedDoc() {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id: ID, usedBy: ['default'] },
    ),
  )
}

async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfile: () => freeDoc(), ...options })
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

// ── 22 L2.B2: the reason floats, it is not a line of the bar ─────────────

test('22 L2.B2 c (replacing 22 D4.14’s line, 22 D4 review D1): the reason of the bar’s Delete that cannot act floats over the page: the tip is the Delete’s next sibling in the box that owns the hover and the focus, the box stands in the bar, and the sheet takes the tip out of the flow, so no reason adds a line to the bar and the bar’s height never changes', async () => {
  const { main } = await loaded({ modelProfile: () => usedDoc() })
  const del = deleteButton(main)
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').not.toBeNull()
  const reason = main.querySelector('#model-profile-delete-reason')
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

test('22 D4.10 (as 22 IP5.3 amends what disables it): the Delete’s state does not follow the form’s dirty state: an edit leaves the active Delete active on a profile no instance profile lists, and the disabled one disabled on a profile one does', async () => {
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
  const on = await loaded({ modelProfile: () => usedDoc() })
  expect(
    deleteButton(on.main).getAttribute('aria-disabled'),
    '22 IP5.3: disabled at rest, an instance profile listing it',
  ).toBe('true')
  makeDirty(on.main)
  expect(
    deleteButton(on.main).getAttribute('aria-disabled'),
    '22 D4.10: still disabled with the page holding an edit',
  ).toBe('true')
})
