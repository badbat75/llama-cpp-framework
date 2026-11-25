// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4): the delete
// of a server profile is a − in the Used by cell of each row of the Server
// profiles list, after the value. This file is 22 D4.1 to D4.8 for that list:
// the button, when it can act, its hint, the confirm row it opens, one delete at
// a time, the answers of the manager, and the widths. The Model profiles list is
// ModelProfilesDelete1.test.tsx, the pages are ModelProfileDelete3.test.tsx. The
// list page, its table and the create form stay ServerProfiles1 to 3.
//
// 22 D4.8's measurements (320 px, 200 % text, no sideways scroll, the − wrapping
// under the value, the group wrapping inside the table's scroll box) are the
// `fe/ux` check's; what the cell, the − and the reason must declare is pinned
// here as the sheet text, the way 17b2.2 does. 22 D4.13, the review's D1
// decision, is pinned here too as the 22D4 amendment leaves it: the hint box
// owns the pointer handlers and the tip is the −'s next sibling inside it. The
// tip floats since that amendment (22 D4.16 to D4.20), placed by a script
// against the viewport: 22 D4.16 to D4.22 are ListDeleteTip.test.tsx. The
// helpers and the stylesheet reader are local copies: a test file imports no
// other test file.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
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
  deferred,
  en,
  fakeBff,
  isHidden,
  reply,
  shownText,
  startAt,
  stubDialog,
  type BffOptions,
  type ServerProfile,
} from './test/bff.ts'

/** `trash-2` of `Icon.tsx`: lucide-static 1.74.0's icons/trash-2.svg, the five
 *  paths in the order the package writes them (22 D4.21). */
const TRASH = [
  'M10 11v6',
  'M14 11v6',
  'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
  'M3 6h18',
  'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
]
const PLUS_VERTICAL = 'M12 5v14'

/** One row of GET /api/profiles/server. */
function row(
  id: string,
  instance: string | null = 'default',
  port = 8580,
  tags: string[] = [],
): ServerProfile {
  return { id, engine: 'llama.cpp', port, tags, instance }
}

/** One profile an instance runs, one free: only the free row's − can act. */
const ROWS = [row('default', 'default'), row('qwen-27b', null, 9931)]

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/profiles/server')
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

/** The app at #/profiles/server, the pool read. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ serverProfiles: ROWS, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('server-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('server-profiles-title') })
}

function dataRows(main: HTMLElement): HTMLElement[] {
  return within(table(main))
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

function cellsOf(row: Element): Element[] {
  return [...row.querySelectorAll('td, th')]
}

function rowOf(main: HTMLElement, id: string): HTMLElement {
  const row = dataRows(main).find(
    (r) => (cellsOf(r)[0]?.textContent ?? '').trim() === id,
  )
  expect(row, `the row of ${id}`).not.toBeUndefined()
  return row!
}

/** The Used by cell, the last of the four (17a.3). */
function usedByCell(main: HTMLElement, id: string): HTMLElement {
  return cellsOf(rowOf(main, id))[3] as HTMLElement
}

/** The row's −, named by `server-profiles-delete-row` with the id (22 D4.1). */
function minusOf(main: HTMLElement, id: string): HTMLElement {
  return within(table(main)).getByRole('button', {
    name: en('server-profiles-delete-row', id),
  })
}

/** The box that holds the row's − and the reason it shows (22 D4.13). */
function boxOf(main: HTMLElement, id: string): HTMLElement {
  const minus = minusOf(main, id)
  const box = minus.parentElement
  expect(box, 'the − sits in a box of its own').not.toBeNull()
  return box!
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function confirmRow(main: HTMLElement): Element | null {
  return main.querySelector('tr.confirm-row')
}

const question = (id: string) => en('server-profiles-delete-question', id)

function queryConfirm(main: HTMLElement, id: string): HTMLElement | null {
  return within(main).queryByRole('group', { name: question(id) })
}

async function confirmGroup(
  main: HTMLElement,
  id: string,
): Promise<HTMLElement> {
  press(minusOf(main, id))
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

function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll<HTMLElement>('[role="alert"]')]
    .map((a) => text(a))
    .filter((s) => s !== '')
}

/** The region line 22 D4.6 adds above the table: a `role="status"` before it. */
function regionStatus(main: HTMLElement): Element | null {
  const lines = [...main.querySelectorAll('[role="status"]')].filter((el) =>
    before(el, table(main)),
  )
  expect(lines.length, 'one status line above the table').toBe(1)
  return lines[0] ?? null
}

const deleteRoute = (id: string) => `/api/profiles/server/${id}`

// ── The stylesheet reader ───────────────────────────────────────────────

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

// ── 22 D4.1: the − on every row ─────────────────────────────────────────

test('22 D4.1 (as 22 D4.21 changes its icon): every row has a − in its Used by cell after the value, in the shared 40 px .ot-add square with flex: none and the ot-delete class, the aria-hidden trash-2 icon and a visually hidden server-profiles-delete-row naming it with the row id, in a cell that is a wrapping flex row with gap var(--space-3) and align-items center', async () => {
  const { main } = await loaded()
  const rs = dataRows(main)
  expect(rs, 'one row per profile').toHaveLength(ROWS.length)
  for (const r of rs) {
    const id = text(cellsOf(r)[0])
    const cell = usedByCell(main, id)
    const minus = minusOf(main, id)
    expect(minus.closest('td'), '22 D4.1: the − is in the Used by cell').toBe(
      cell,
    )
    const value = [...cell.children].find((el) => el !== minus) as
      HTMLElement | undefined
    expect(value, 'the value is there, before the −').not.toBeUndefined()
    expect(
      value!.compareDocumentPosition(minus) & Node.DOCUMENT_POSITION_FOLLOWING,
      '22 D4.1: after the value',
    ).not.toBe(0)
    // 17a.3: the value the cell shows before the − is the instance's link for
    // a profile an instance runs, the muted "No instance" for a free one.
    const instance = ROWS.find((x) => x.id === id)?.instance
    if (instance === null || instance === undefined)
      expect(text(value!), '17a.3: a free profile says No instance').toBe(
        en('server-profiles-unused'),
      )
    else
      expect(
        within(value!).queryByRole('link', { name: instance }),
        '17a.3: the value is the instance link',
      ).not.toBeNull()
    expect(text(value!), 'the value is the one 17a.3 shows').not.toBe('')
    expect(minus.tagName, 'a native button').toBe('BUTTON')
    expect(minus.getAttribute('type'), 'it submits nothing').toBe('button')
    expect(minus.getAttribute('aria-expanded'), 'a disclosure, closed').toBe(
      'false',
    )
    const svg = minus.querySelector('svg')
    expect(svg, 'the icon').not.toBeNull()
    expect(svg!.getAttribute('aria-hidden'), 'decorative').toBe('true')
    const paths = [...svg!.querySelectorAll('path')].map((p) =>
      p.getAttribute('d'),
    )
    expect(paths, '22 D4.21: the trash can, in order').toEqual(TRASH)
    expect(paths, 'and no vertical stroke: the − is no plus').not.toContain(
      PLUS_VERTICAL,
    )
    expect(minus.classList.contains('ot-add'), 'the shared icon button').toBe(
      true,
    )
    expect(
      minus.classList.contains('ot-delete'),
      '22 D4.21: a delete carries its own class beside it',
    ).toBe(true)
    expect(
      text(minus.querySelector('span.visually-hidden')),
      '4.1.2, 2.4.6: the name is the row’s own',
    ).toBe(en('server-profiles-delete-row', id))
    expect(minus.getAttribute('aria-label'), 'named by the span').toBeNull()
    expect(shownText(minus), '2.5.3: nothing is written on the button').toBe('')
    const box = cascade(minus).decls
    expect(norm(box['width'] ?? ''), 'the shared 40 px square').toBe('40px')
    expect(norm(box['min-height'] ?? ''), 'and never shorter').toBe('40px')
    expect(norm(box['flex'] ?? ''), 'kept at its box').toBe('none')
    expect(
      norm(box['border'] ?? ''),
      'the --control outline of a control',
    ).toBe('var(--border-w) solid var(--control)')
    const cellBox = cascade(cell).decls
    expect(
      norm(cellBox['display'] ?? ''),
      '22 D4.1: the cell is a flex row',
    ).toBe('flex')
    expect(norm(cellBox['flex-wrap'] ?? ''), 'that wraps').toBe('wrap')
    expect(norm(cellBox['gap'] ?? ''), 'with this space between the two').toBe(
      'var(--space-3)',
    )
    expect(norm(cellBox['align-items'] ?? ''), 'centred on the line').toBe(
      'center',
    )
  }
})

// ── 22 D4.2: enabled only when deletable ────────────────────────────────

test('22 D4.2: the − of a row no instance uses is active; a row an instance uses carries aria-disabled and aria-describedby to a visually hidden span with server-profiles-delete-in-use, a press sends nothing and opens nothing, and the state follows each list read', async () => {
  const { bff, main } = await loaded()
  const free = minusOf(main, 'qwen-27b')
  expect(inactive(free), 'a free profile: active').toBe(false)
  expect(free.getAttribute('aria-describedby'), 'no reason needed').toBeNull()
  const used = minusOf(main, 'default')
  expect(inactive(used), '22 D4.2: an instance uses it: aria-disabled').toBe(
    true,
  )
  expect(
    used.hasAttribute('disabled'),
    'aria-disabled, not disabled: it stays focusable',
  ).toBe(false)
  const reason = document.getElementById(used.getAttribute('aria-describedby')!)
  expect(reason, 'the reason is its description').not.toBeNull()
  expect(text(reason), 'the words of the brief').toBe(
    en('server-profiles-delete-in-use'),
  )
  expect(isHidden(reason), 'visually hidden').toBe(true)
  press(used)
  await flushTasks()
  expect(bff.to('DELETE', deleteRoute('default')), 'nothing sent').toHaveLength(
    0,
  )
  expect(queryConfirm(main, 'default'), 'nothing opened').toBeNull()
  expect(confirmRow(main), 'and no confirm row').toBeNull()
  // It follows the read: the re-read that frees `default` frees its −.
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/server')
  const freed = await loaded({
    serverProfiles: () => reply(200, [row('default', null), ROWS[1]!]),
  })
  expect(
    inactive(minusOf(freed.main, 'default')),
    '22 D4.2: no instance uses it, so its − can act',
  ).toBe(false)
})

// ── 22 D4.3, D4.13: the reason beside the − ─────────────────────────────

test('22 D4.3, D4.13, D4.18 (as 22 D4.16 to D4.18 amend them): hovering the − of a profile an instance uses, and focusing it, shows its reason as a .hint-tip box floating over the table (a --surface fill, a --control border, at most min(16rem, 100vw - 16px) wide, wrapping, never over the button, placed by 22 D4.17); it stays while the pointer is over it, the pointer leaving the box hides it, Escape hides it and keeps the focus on the −, a tap on the inactive − shows it and a pointerdown outside the box hides it, and an active − shows nothing', async () => {
  const { main } = await loaded()
  const box = boxOf(main, 'default')
  const minus = minusOf(main, 'default')
  // The reason the − names as its description (22 D4.2), kept hidden.
  const said = document.getElementById(minus.getAttribute('aria-describedby')!)
  expect(said, '22 D4.2: the − names its reason').not.toBeNull()
  // The reason the eye reads: a line of the box, not the hidden description.
  const shown = () =>
    Array.from(box.querySelectorAll('span')).filter(
      (s) => !isHidden(s) && text(s) === en('server-profiles-delete-in-use'),
    )
  expect(shown(), 'nothing shows at rest').toHaveLength(0)
  fireEvent.mouseOver(minus)
  await flushTasks()
  expect(shown(), '22 D4.3: the hover shows the reason').toHaveLength(1)
  const line = shown()[0]!
  expect(line, '22 D4.13: it is not the hidden description').not.toBe(said)
  expect(box.contains(line), 'in the box that holds the −').toBe(true)
  expect(minus.contains(line), 'the − does not wrap the reason').toBe(false)
  expect(
    line.closest('button'),
    'the reason is not inside the button',
  ).toBeNull()
  const look = cascade(line).decls
  expect(norm(look['background'] ?? ''), 'the --surface fill').toBe(
    'var(--surface)',
  )
  expect(
    `${norm(look['border'] ?? '')} ${norm(look['border-color'] ?? '')}`,
    'a --control border',
  ).toContain('var(--control)')
  expect(
    (look['max-width'] ?? '').replace(/\s+/g, ''),
    '22 D4.17: at most 16rem, and never past the gutters at 320 px (1.4.10)',
  ).toBe('min(16rem,100vw-16px)')
  expect(norm(look['white-space'] ?? 'normal'), 'and it wraps').not.toBe(
    'nowrap',
  )
  expect(
    look['font-size'] === undefined,
    'the body text size: it declares no font-size of its own',
  ).toBe(true)
  // The pointer moves from the − to the reason: 1.4.13, it stays. The reason is
  // inside the box, so the pointer never leaves it.
  fireEvent.mouseOut(minus, { relatedTarget: line })
  fireEvent.mouseOver(line)
  await flushTasks()
  expect(shown(), 'kept while over the reason').toHaveLength(1)
  // The pointer leaves the box: the reason goes.
  fireEvent.mouseOut(box, { relatedTarget: document.body })
  await flushTasks()
  expect(shown(), 'gone when the pointer leaves the box').toHaveLength(0)
  // 22 D4.13: the hint box owns the pointer handlers, not the −: a pointer over
  // the box, beside the −, shows the reason too.
  fireEvent.mouseOver(box)
  await flushTasks()
  expect(shown(), '22 D4.13: the box shows it, not only the −').toHaveLength(1)
  fireEvent.mouseLeave(box)
  await flushTasks()
  expect(shown(), 'and a mouseleave of the box hides it').toHaveLength(0)
  // Keyboard: the focus on the − shows it, Escape and the focus leaving hide it.
  minus.focus()
  await flushTasks()
  expect(shown(), '22 D4.3: the focus shows it').toHaveLength(1)
  fireEvent.keyDown(minus, { key: 'Escape' })
  await flushTasks()
  expect(shown(), '22 D4.13: Escape hides it').toHaveLength(0)
  expect(
    document.activeElement,
    '22 D4.18: Escape keeps the focus on the −, it sends the user nowhere',
  ).toBe(minus)
  // Escape leaves the focus on the −, and a focus() on an element that already
  // has it fires no focus event (probed): the focus has to leave and come back
  // for the reason to show again (22 D4.13: shown on focus, hidden on blur).
  minus.blur()
  await flushTasks()
  minus.focus()
  await flushTasks()
  expect(shown(), 'and it comes back on focus').toHaveLength(1)
  minus.blur()
  await flushTasks()
  expect(shown(), 'and it goes with the focus').toHaveLength(0)
  // 22 D4.18: touch. A tap on the inactive − shows the reason (its click does
  // nothing else), and a pointerdown outside the box hides it.
  fireEvent.pointerDown(minus, { pointerType: 'touch' })
  fireEvent.click(minus)
  await flushTasks()
  expect(shown(), '22 D4.18: a tap on the inactive − shows it').toHaveLength(1)
  fireEvent.pointerDown(document.body)
  await flushTasks()
  expect(
    shown(),
    '22 D4.18: a pointerdown outside the box hides it',
  ).toHaveLength(0)
  // An active − shows nothing: its name says what it does.
  const active = minusOf(main, 'qwen-27b')
  fireEvent.mouseOver(active)
  active.focus()
  await flushTasks()
  expect(
    boxOf(main, 'qwen-27b').children,
    '22 D4.3: an active − shows nothing beside it',
  ).toHaveLength(1)
})

// ── 22 D4.13, D4.16: the reason floats (22D4 review D1, amended) ────────

test('22 D4.13, D4.16 (22 D4 review D1, as the 22D4 amendment amends it): the − and its reason are siblings in the .hint-box of the cell, the − first, and the rule that styles the reason places it against the viewport (position: fixed), so showing it changes no layout', async () => {
  const { main } = await loaded()
  const cell = usedByCell(main, 'default')
  const box = boxOf(main, 'default')
  const minus = minusOf(main, 'default')
  expect(box.parentElement, 'the box sits in the cell, where the − sat').toBe(
    cell,
  )
  fireEvent.mouseOver(minus)
  await flushTasks()
  const line = Array.from(box.querySelectorAll('span')).find(
    (s) => !isHidden(s) && text(s) === en('server-profiles-delete-in-use'),
  )
  expect(line, 'the hover shows the reason').not.toBeUndefined()
  expect(line!.parentElement, 'the − and the reason are siblings').toBe(box)
  expect(
    line!.getAttribute('aria-hidden'),
    '22 D4.13: it is the eye’s copy: the − names the hidden one',
  ).toBe('true')
  expect(
    box.firstElementChild,
    '22 D4.13: the − comes first, so the reason starts under it',
  ).toBe(minus)
  // 22 D4.16: the reason floats. `position: fixed` takes it out of the cell's
  // flow, so showing it changes no row height, no column width and no scroll
  // box: the amendment's replacement for the review's "in the flow". The
  // measurements are the designer's, the declaration is pinned here.
  const decls = TOP.filter((r) =>
    r.compact.split(',').includes('.hint-tip'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(decls.length, 'the reason has rules in the sheet').toBeGreaterThan(0)
  expect(
    decls.filter((d) => d.startsWith('position:')),
    '22 D4.16: the reason is placed against the viewport and not against the cell, so the table’s scroll box neither clips it nor counts it',
  ).toEqual(['position: fixed'])
  expect(
    cascade(line!).decls['position'],
    '22 D4.16: the rule that reaches the shown reason says fixed',
  ).toBe('fixed')
})

// ── 22 D4.4: the confirm row ────────────────────────────────────────────

test('22 D4.4: pressing an active − inserts right after the row a tr.confirm-row with one td spanning the four columns, holding the group named by its server-profiles-delete-question paragraph, confirm-delete then confirm-cancel, an empty status line and no alert; the − takes aria-expanded true and the focus goes to Cancel; Cancel and Escape remove the row and focus the −; another row’s confirm closes the first', async () => {
  const { bff, main } = await loaded({
    serverProfiles: [row('qwen-27b', null, 9931), row('free-2', null, 9932)],
  })
  const r = rowOf(main, 'qwen-27b')
  const columns = cellsOf(r).length
  const minus = minusOf(main, 'qwen-27b')
  const group = await confirmGroup(main, 'qwen-27b')
  const tr = confirmRow(main)
  expect(tr, '22 D4.4: the confirm row').not.toBeNull()
  expect(tr!.previousElementSibling, 'right after the profile’s row').toBe(r)
  expect(tr!.parentElement, 'in the table’s body').toBe(r.parentElement)
  const tds = [...tr!.querySelectorAll('td')]
  expect(tds, 'one td').toHaveLength(1)
  expect(Number(tds[0]!.getAttribute('colspan')), 'spanning every column').toBe(
    columns,
  )
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  expect(tds[0]!.contains(group), 'the group is in the td').toBe(true)
  const asked = document.getElementById(group.getAttribute('aria-labelledby')!)
  expect(asked, 'named by its paragraph').not.toBeNull()
  expect(asked!.tagName, 'a paragraph').toBe('P')
  expect(text(asked), 'the list’s own question').toBe(question('qwen-27b'))
  expect(group.contains(asked), 'the question is in the group').toBe(true)
  expect(
    within(group)
      .getAllByRole('button')
      .map((b) => (b.textContent ?? '').trim()),
    'the verb then Cancel',
  ).toEqual([en('confirm-delete'), en('confirm-cancel')])
  expect(groupStatus(group).textContent, 'the status line is empty').toBe('')
  expect(alertTexts(group), 'no alert: the place is empty').toEqual([])
  expect(minus.getAttribute('aria-expanded'), 'the − says it is open').toBe(
    'true',
  )
  const cancel = confirmIn(group, 'confirm-cancel')
  expect(document.activeElement, 'the focus is on Cancel').toBe(cancel)
  expect(
    bff.to('DELETE', deleteRoute('qwen-27b')),
    'nothing sent',
  ).toHaveLength(0)
  press(cancel)
  await flushTasks()
  expect(confirmRow(main), 'Cancel removes the row').toBeNull()
  expect(document.activeElement, 'the focus is back on the −').toBe(minus)
  expect(minus.getAttribute('aria-expanded'), 'closed').toBe('false')
  fireEvent.keyDown(minus, { key: 'Escape' })
  await flushTasks()
  const again = await confirmGroup(main, 'qwen-27b')
  expect(document.activeElement, 'Escape closed it, open again').toBe(
    confirmIn(again, 'confirm-cancel'),
  )
  fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' })
  await flushTasks()
  expect(confirmRow(main), 'Escape closes it').toBeNull()
  expect(document.activeElement, 'on the −').toBe(minus)
  // One at a time.
  const first = await confirmGroup(main, 'qwen-27b')
  const second = await confirmGroup(main, 'free-2')
  expect(confirmRow(main), 'one confirm row').not.toBeNull()
  expect(
    confirmRow(main)!.previousElementSibling,
    '22 D4.4: the second row’s',
  ).toBe(rowOf(main, 'free-2'))
  expect(second.isConnected && first.isConnected, 'the first is gone').not.toBe(
    true,
  )
  expect(
    minusOf(main, 'qwen-27b').getAttribute('aria-expanded'),
    'and its − says closed',
  ).toBe('false')
  expect(document.activeElement, 'the focus is on the new Cancel').toBe(
    confirmIn(second, 'confirm-cancel'),
  )
})

// ── 22 D4.5: one delete at a time ───────────────────────────────────────

test('22 D4.5: while the delete runs the group’s status reads confirm-deleting with its buttons aria-disabled, every row’s − is aria-disabled, and a second press sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfileDelete: () => gate.promise,
  })
  const group = await confirmGroup(main, 'qwen-27b')
  press(confirmIn(group, 'confirm-delete'))
  await within(group).findByText(en('confirm-deleting'))
  expect(
    bff.to('DELETE', deleteRoute('qwen-27b')),
    'the verb went once',
  ).toHaveLength(1)
  for (const key of ['confirm-delete', 'confirm-cancel'])
    expect(inactive(confirmIn(group, key)), `${key} is aria-disabled`).toBe(
      true,
    )
  for (const p of ROWS)
    expect(inactive(minusOf(main, p.id)), `${p.id}’s − is aria-disabled`).toBe(
      true,
    )
  press(confirmIn(group, 'confirm-delete'))
  press(minusOf(main, 'qwen-27b'))
  await flushTasks()
  expect(
    bff.to('DELETE', deleteRoute('qwen-27b')),
    '22 D4.5: a second press sends nothing',
  ).toHaveLength(1)
  gate.resolve(reply(204))
  await within(main).findByText(en('server-profiles-deleted', 'qwen-27b'))
})

// ── 22 D4.6: the region's line ──────────────────────────────────────────

test('22 D4.6: the delete region has one line above the table, there from the first render and empty at rest, before the table and its scrolling box, and the list’s + stays the last thing of the list', async () => {
  // The line is the region's, not a toast: it is there to be read from, so it
  // is in the DOM at rest and empty, as the switch region's line is on the
  // Models list (2c.50). It stands between the intro and the table, which is
  // the change 17b2.1 records in ServerProfiles3.test.tsx.
  const { main } = await loaded()
  const line = regionStatus(main)
  expect(line, 'one status line above the table').not.toBeNull()
  expect(text(line), 'empty at rest').toBe('')
  const box = table(main).closest('div.table-scroll')
  expect(box, 'the table is in its scrolling box').not.toBeNull()
  expect(before(line!, box!), 'and the line is above it').toBe(true)
  expect(
    within(main).queryAllByRole('alert'),
    'the region’s alert place is empty at rest: the page holds no alert',
  ).toHaveLength(0)
  const opener = within(main).getByRole('button', {
    name: en('server-profiles-new'),
  })
  expect(before(box!, opener), 'the + stays after the table').toBe(true)
})

// ── 22 D4.6: done ───────────────────────────────────────────────────────

test('22 D4.6: a 204 removes the profile’s row and the confirm row, the region line above the table says server-profiles-deleted with the id, the focus goes to the next row’s −, else the previous row’s, else the list’s +, and the list is not read again', async () => {
  const { bff, main } = await loaded()
  const group = await confirmGroup(main, 'qwen-27b')
  const reads = bff.to('GET', '/api/profiles/server').length
  press(confirmIn(group, 'confirm-delete'))
  const done = await within(main).findByText(
    en('server-profiles-deleted', 'qwen-27b'),
  )
  expect(
    dataRows(main).map((r) => text(cellsOf(r)[0])),
    'the row is gone',
  ).toEqual(['default'])
  expect(confirmRow(main), 'and so is the confirm row').toBeNull()
  expect(done.getAttribute('role'), '22 D4.6: a status line').toBe('status')
  expect(before(done, table(main)), 'above the table').toBe(true)
  expect(done, 'the region’s line, not a new one').toBe(regionStatus(main))
  expect(
    bff.to('GET', '/api/profiles/server').length,
    '22 D4.6: the list was not read again',
  ).toBe(reads)
  expect(
    document.activeElement,
    'the last row is gone: the focus is on the previous row’s −',
  ).toBe(minusOf(main, 'default'))
  // The next row’s − when there is one.
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/server')
  const second = await loaded({
    serverProfiles: [row('qwen-27b', null, 9931), row('default', 'default')],
  })
  const g2 = await confirmGroup(second.main, 'qwen-27b')
  press(confirmIn(g2, 'confirm-delete'))
  await within(second.main).findByText(
    en('server-profiles-deleted', 'qwen-27b'),
  )
  expect(
    document.activeElement,
    '22 D4.6: the focus is on the next row’s −',
  ).toBe(minusOf(second.main, 'default'))
  // The list’s + when no row is left (22 D4.6).
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/server')
  const third = await loaded({ serverProfiles: [row('qwen-27b', null, 9931)] })
  const g3 = await confirmGroup(third.main, 'qwen-27b')
  press(confirmIn(g3, 'confirm-delete'))
  await within(third.main).findByText(en('server-profiles-deleted', 'qwen-27b'))
  expect(dataRows(third.main), 'the pool is empty').toHaveLength(0)
  expect(document.activeElement, '22 D4.6: the focus is on the list’s +').toBe(
    document.getElementById('server-profiles-new'),
  )
})

// ── 22 D4.7: refusals ───────────────────────────────────────────────────

test('22 D4.7: a 409 client-server-profile-in-use reads the list again, the confirm row goes, the row shows the profile used by the instance with its − disabled and focusable, api-client-server-profile-in-use is in the region’s alert above the table and the focus is on the row’s −', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    serverProfiles: () =>
      reply(
        200,
        ++reads === 1 ? ROWS : [ROWS[0]!, row('qwen-27b', 'coder', 9931)],
      ),
    serverProfileDelete: () =>
      reply(409, {
        key: 'client-server-profile-in-use',
        args: ['qwen-27b', 'coder'],
      }),
  })
  const refusal = en('api-client-server-profile-in-use', 'qwen-27b', 'coder')
  const group = await confirmGroup(main, 'qwen-27b')
  press(confirmIn(group, 'confirm-delete'))
  const kept = await within(main).findByText(refusal)
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(bff.to('GET', '/api/profiles/server').length, 'twice').toBe(2)
  expect(confirmRow(main), 'the confirm row is gone').toBeNull()
  expect(queryConfirm(main, 'qwen-27b'), 'and so is the group').toBeNull()
  expect(kept.getAttribute('role'), 'the region’s alert').toBe('alert')
  expect(before(kept, table(main)), 'above the table').toBe(true)
  const minus = minusOf(main, 'qwen-27b')
  expect(
    text(usedByCell(main, 'qwen-27b')),
    'the row names the instance that uses it',
  ).toContain('coder')
  expect(inactive(minus), 'with its − disabled').toBe(true)
  expect(document.activeElement, '22 D4.7: the focus is on the −').toBe(minus)
})

test('22 D4.7: a 404 client-no-server-profile reads the list again, puts api-client-no-server-profile in the region’s alert above the table, and moves the focus as after a 204', async () => {
  let reads = 0
  const { main } = await loaded({
    serverProfiles: () => reply(200, ++reads === 1 ? ROWS : [ROWS[0]!]),
    serverProfileDelete: () =>
      reply(404, { key: 'client-no-server-profile', args: [] }),
  })
  const group = await confirmGroup(main, 'qwen-27b')
  press(confirmIn(group, 'confirm-delete'))
  const kept = await within(main).findByText(en('api-client-no-server-profile'))
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(kept.getAttribute('role'), 'the region’s alert').toBe('alert')
  expect(before(kept, table(main)), 'above the table').toBe(true)
  expect(confirmRow(main), 'the confirm row is gone').toBeNull()
  expect(
    dataRows(main).map((r) => text(cellsOf(r)[0])),
    'the row is gone with the file',
  ).toEqual(['default'])
  expect(
    document.activeElement,
    '22 D4.7: the focus moved as after a 204',
  ).toBe(minusOf(main, 'default'))
})

test('22 D4.7 (22 D4 review D4): opening a confirm clears the region’s alert: after a 404 leaves api-client-no-server-profile above the table, the next row’s confirm opens with no alert, and its successful delete leaves the was-deleted line alone', async () => {
  let reads = 0
  const { main } = await loaded({
    serverProfiles: () =>
      reply(
        200,
        ++reads === 1
          ? ROWS
          : [row('default', null), row('free-2', null, 9932)],
      ),
    serverProfileDelete: (id) =>
      id === 'qwen-27b'
        ? reply(404, { key: 'client-no-server-profile', args: [] })
        : reply(204),
  })
  const group = await confirmGroup(main, 'qwen-27b')
  press(confirmIn(group, 'confirm-delete'))
  await within(main).findByText(en('api-client-no-server-profile'))
  await flushTasks()
  expect(
    alertTexts(main),
    'the 404 leaves its alert above the table',
  ).toHaveLength(1)
  // The next delete of the same region: opening its confirm clears the old
  // alert, so the was-deleted line of the success stands alone (D4).
  const next = await confirmGroup(main, 'default')
  expect(confirmRow(main), 'the confirm row is open').not.toBeNull()
  expect(
    alertTexts(main),
    '22 D4 review D4: opening a confirm clears the region’s alert',
  ).toEqual([])
  press(confirmIn(next, 'confirm-delete'))
  const done = await within(main).findByText(
    en('server-profiles-deleted', 'default'),
  )
  expect(done.getAttribute('role'), 'the region’s line says it').toBe('status')
  expect(alertTexts(main), 'and no old alert sits beside it').toEqual([])
})

test('22 D4.7: a 504 client-timeout shows delete-unknown in the group, then reads the list again: the row gone, the same text moves to the region’s alert and the focus moves as after a 204; a 503 keeps its text in the group’s alert with the focus on confirm-delete', async () => {
  // Gone.
  let reads = 0
  const { main } = await loaded({
    serverProfiles: () => reply(200, ++reads === 1 ? ROWS : [ROWS[0]!]),
    serverProfileDelete: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  const group = await confirmGroup(main, 'qwen-27b')
  press(confirmIn(group, 'confirm-delete'))
  const said = await within(group).findByText(en('delete-unknown'))
  expect(said.closest('[role="alert"]'), '504: delete-unknown').not.toBeNull()
  expect(group.contains(said), 'first in the group').toBe(true)
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(
    dataRows(main).map((r) => text(cellsOf(r)[0])),
    'the row is gone',
  ).toEqual(['default'])
  expect(confirmRow(main), 'the confirm row went with it').toBeNull()
  const moved = within(main).getByText(en('delete-unknown'))
  expect(moved.getAttribute('role'), 'it moved to the region’s alert').toBe(
    'alert',
  )
  expect(before(moved, table(main)), 'above the table').toBe(true)
  expect(document.activeElement, 'and the focus moved as after a 204').toBe(
    minusOf(main, 'default'),
  )
  // The group keeps a plain refusal.
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/server')
  const other = await loaded({
    serverProfileDelete: () =>
      reply(503, { key: 'client-unavailable', args: [] }),
  })
  const g2 = await confirmGroup(other.main, 'qwen-27b')
  const verb = confirmIn(g2, 'confirm-delete')
  press(verb)
  const alert = await within(g2).findByText(en('api-client-unavailable'))
  expect(group.contains(alert), 'the alert is the group’s').toBe(false)
  expect(g2.contains(alert), '503: in the group’s alert').toBe(true)
  expect(document.activeElement, 'the focus stays on confirm-delete').toBe(verb)
  expect(
    other.bff.to('GET', '/api/profiles/server').length,
    '22 D4.7: a plain refusal reads the list once',
  ).toBe(1)
})

// ── 22 D4.8: widths ─────────────────────────────────────────────────────

test('22 D4.8: the confirm row is a row of the table, so its group sits inside the table’s scroll box and wraps with it: the td declares no width of its own and the group no width, and the Used by cell keeps its wrap', async () => {
  const { main } = await loaded()
  const group = await confirmGroup(main, 'qwen-27b')
  const tr = confirmRow(main)!
  const td = tr.querySelector('td')!
  const box = table(main).closest('.table-scroll')
  expect(box, 'the table is in its scrolling box').not.toBeNull()
  expect(box!.contains(tr), '22 D4.8: the confirm row is inside it').toBe(true)
  expect(
    cascade(td as HTMLElement).decls['width'],
    'the td takes the box’s width: it declares none',
  ).toBeUndefined()
  expect(
    cascade(group).decls['width'],
    'the group follows the td: it declares no width of its own',
  ).toBeUndefined()
  expect(
    norm(cascade(group).decls['min-width'] ?? 'none'),
    'and shrinks below its content, so the words wrap (1.4.10)',
  ).toBe('0')
  expect(
    norm(cascade(usedByCell(main, 'qwen-27b')).decls['flex-wrap'] ?? ''),
    '22 D4.8: the − wraps under the value inside its cell',
  ).toBe('wrap')
})
