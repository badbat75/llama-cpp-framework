// Brief 22, slice 22D4 (fe/briefs/22-model-sources.md, section 22D4): the delete
// of a model profile is a − in the Status cell of each row of the Model profiles
// list, beside the switch. 22 D4.1 to D4.8 are this file: the button, when it
// can act, its hint, the confirm row it opens, one delete at a time, the answer
// of the manager, and the widths. The pages (22 D4.9 to D4.11) are
// ModelProfileDelete3.test.tsx; the answers of 22 D.2 to D.7 that the confirm
// keeps are pinned there and in ModelProfileDelete.test.tsx.
//
// 22 D4.8's widths (320 px, 200 % text, no sideways scroll, the − wrapping under
// the switch, the group wrapping inside the table's scroll box) are
// measurements: jsdom lays nothing out, so what the cell, the − and the reason
// must declare is pinned here as the sheet text, the way 22 D2.2 and 17b2.2 do.
// The measurements themselves are the `fe/ux` check's states, which 22 D4.12
// says gain `model-profiles-delete` (a confirm row open) and the disabled −'s
// reason. 22 D4.13, the review's D1 decision, is pinned here too as the 22D4
// amendment leaves it: the hint box owns the pointer handlers and the tip is
// the −'s next sibling inside it. The tip floats since that amendment (22 D4.16
// to D4.20), placed by a script against the viewport: 22 D4.16 to D4.22 are
// ListDeleteTip.test.tsx, which reads both lists.
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
  rowName,
  shownText,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

/** `trash-2` of `Icon.tsx`: lucide-static 1.74.0's icons/trash-2.svg, the five
 *  paths in the order the package writes them (22 D4.21: the row's − is a trash
 *  can, a stored profile being a file). */
const TRASH = [
  'M10 11v6',
  'M14 11v6',
  'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
  'M3 6h18',
  'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
]
const PLUS_VERTICAL = 'M12 5v14'

function modelProfile(
  id: string,
  enabled: boolean,
  model = '',
  engine = 'llama.cpp',
  engineAvailable = true,
): ModelProfile {
  return { id, engine, engineAvailable, enabled, model }
}

/** Two on, one off, in the bff's order: only the last row's − can act. */
const LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  modelProfile('Coder-30B', true, '/m/coder/Coder-30B.gguf'),
  modelProfile('Old-Coder', false),
]

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/profiles/model')
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a promise chain) run. */
async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

/** The app at #/profiles/model, the table loaded. */
async function open(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('model-profiles-title') })
}

/** The rows that carry data, in the reply order. */
function dataRows(main: HTMLElement): HTMLElement[] {
  return within(table(main))
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

function cellsOf(row: Element): Element[] {
  return [...row.querySelectorAll('td, th')]
}

/** The row's name as the row is looked up and as its controls are named: the
 *  id alone. 22 D4.35 adds the engine as a muted line under the name, and the
 *  designer's ruling of 2026-11-27 keeps that line out of the name. */
function nameOf(row: Element): string {
  return rowName(cellsOf(row)[0]!)
}

/** The row of the profile `id`, by its name cell. */
function rowOf(main: HTMLElement, id: string): HTMLElement {
  const row = dataRows(main).find((r) => nameOf(r) === id)
  expect(row, `the row of ${id}`).not.toBeUndefined()
  return row!
}

/** The row's −, named by `model-profiles-delete-row` with the id (22 D4.1). */
function minusOf(main: HTMLElement, id: string): HTMLElement {
  return within(table(main)).getByRole('button', {
    name: en('model-profiles-delete-row', id),
  })
}

/** The box that holds the row's − and the reason it shows (22 D4.13). */
function boxOf(main: HTMLElement, id: string): HTMLElement {
  const minus = minusOf(main, id)
  const box = minus.parentElement
  expect(box, 'the − sits in a box of its own').not.toBeNull()
  return box!
}

function switches(main: HTMLElement): HTMLInputElement[] {
  return within(table(main)).queryAllByRole('switch') as HTMLInputElement[]
}

function switchOf(main: HTMLElement, id: string): HTMLInputElement {
  return within(table(main)).getByRole('switch', {
    name: id,
  }) as HTMLInputElement
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function filterField(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The words a node shows, whitespace folded. */
function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The confirm row of 22 D4.4, open or not. */
function confirmRow(main: HTMLElement): Element | null {
  return main.querySelector('tr.confirm-row')
}

/** The list's question for `id` (22 D4.4), the page's own being another key. */
const question = (id: string) => en('model-profiles-delete-question', id)

function queryConfirm(main: HTMLElement, id: string): HTMLElement | null {
  return within(main).queryByRole('group', { name: question(id) })
}

/** The confirm group, once the row's − is pressed. */
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

/** The group's status line: the group holds one (22 D4.4). */
function groupStatus(group: HTMLElement): HTMLElement {
  const all = [...group.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the group has one status line').toHaveLength(1)
  return all[0]!
}

/** The text of every `role="alert"` inside `root`. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll<HTMLElement>('[role="alert"]')]
    .map((a) => text(a))
    .filter((s) => s !== '')
}

/** The region above the table: the switch's status line (2c.50), the line 22
 *  D4.6 says names the profile that was deleted. */
function regionStatus(main: HTMLElement): HTMLElement {
  const line = main.querySelector('.model-profiles-switch-status')
  expect(line, 'the switch region line is there').not.toBeNull()
  return line as HTMLElement
}

/** The region's alert line (2c.50), where 22 D4.7 puts a refusal that leaves no
 *  group behind. */
function regionAlert(main: HTMLElement): HTMLElement | null {
  return main.querySelector('.model-profiles-switch-alert')
}

const deleteRoute = (id: string) =>
  `/api/profiles/model/${encodeURIComponent(id)}`

// ── The stylesheet reader (22 D4.1, D4.3, D4.8) ─────────────────────────

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

/** The declarations that reach `el` from the top-level rules, merged in the
 *  cascade's order, with the rules that carry them (the reader of 22 D2.2, a
 *  local copy). A selector with a descendant, a pseudo-class, an attribute or a
 *  combinator is skipped: it needs a tree jsdom does not lay out. The tag, the
 *  classes and the id are read off the page, so the pin follows the names the
 *  coder chose. */
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

test('22 D4.1 (as 22 D4.21 changes its icon, as 22 D4.23 moves it to its own cell): every row has a − in the Actions cell after the switch, in the shared 40 px .ot-add square with flex: none and the ot-delete class, the aria-hidden trash-2 icon and a visually hidden model-profiles-delete-row naming it with the row id, in a cell that is a plain table cell (22 D4.25)', async () => {
  const { main } = await open()
  const rs = dataRows(main)
  expect(rs, 'one row per profile').toHaveLength(LIST.length)
  for (const row of rs) {
    const id = nameOf(row)
    const cells = cellsOf(row)
    const cell = cells[cells.length - 1]!
    const minus = minusOf(main, id)
    expect(
      minus.closest('td'),
      '22 D4.23: the − is in the row’s last cell, the Actions column',
    ).toBe(cell)
    const input = cells[2]!.querySelector('input')!
    expect(
      input.compareDocumentPosition(minus) & Node.DOCUMENT_POSITION_FOLLOWING,
      '22 D4.1: after the switch, which stays in the Status cell',
    ).not.toBe(0)
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
    const hidden = minus.querySelector('span.visually-hidden')
    expect(hidden, 'the words, for a reader').not.toBeNull()
    expect(
      text(hidden),
      '4.1.2: the name is the row’s own, so each row’s name is unique (2.4.6)',
    ).toBe(en('model-profiles-delete-row', id))
    expect(minus.getAttribute('aria-label'), 'named by the span').toBeNull()
    expect(shownText(minus), '2.5.3: nothing is written on the button').toBe('')
    const box = cascade(minus).decls
    expect(norm(box['width'] ?? ''), 'the shared 40 px square').toBe('40px')
    expect(norm(box['min-height'] ?? ''), 'and never shorter').toBe('40px')
    expect(
      norm(box['flex'] ?? ''),
      'kept at its box, so the cell never shrinks it',
    ).toBe('none')
    expect(
      norm(box['border'] ?? ''),
      'the --control outline of a control',
    ).toBe('var(--border-w) solid var(--control)')
    const cellBox = cascade(cell as HTMLElement).decls
    // 22 D4.25 takes the cell out of the flex layout: 22 D4.1's wrapping flex
    // row with its gap and centred line is gone, and the cell centres its
    // content as a table cell does. ListColumns.test.tsx pins the new shape.
    expect(
      norm(cellBox['display'] ?? ''),
      '22 D4.25: the cell is not a flex box',
    ).not.toBe('flex')
    expect(norm(cellBox['flex-wrap'] ?? ''), 'so it wraps nothing').toBe('')
    expect(norm(cellBox['gap'] ?? ''), 'and has no gap of its own').toBe('')
    expect(norm(cellBox['align-items'] ?? ''), 'and no align-items').toBe('')
  }
})

// ── 22 D4.2: enabled only when deletable ────────────────────────────────

test('22 D4.2: the − of a switched-off row is active; a switched-on row’s carries aria-disabled and aria-describedby to a visually hidden span with model-profiles-delete-needs-off, a press sends nothing and opens nothing, and the state follows each switch', async () => {
  const { bff, main } = await open()
  for (const p of LIST) {
    const minus = minusOf(main, p.id)
    if (p.enabled) {
      expect(inactive(minus), `${p.id} is on: aria-disabled`).toBe(true)
      expect(
        minus.hasAttribute('disabled'),
        'aria-disabled, not disabled: it stays focusable (22 D4.2)',
      ).toBe(false)
      const id = minus.getAttribute('aria-describedby')
      expect(id, 'the reason is its description').toBeTruthy()
      const reason = document.getElementById(id!)
      expect(reason, 'an element the id names').not.toBeNull()
      expect(text(reason), 'the words of the brief').toBe(
        en('model-profiles-delete-needs-off'),
      )
      expect(isHidden(reason), 'visually hidden').toBe(true)
    } else {
      expect(inactive(minus), `${p.id} is off: active`).toBe(false)
      expect(
        minus.getAttribute('aria-describedby'),
        'an active − needs no reason',
      ).toBeNull()
    }
  }
  // A press of a disabled −: nothing.
  press(minusOf(main, 'Qwen3-27B'))
  await flushTasks()
  expect(
    bff.to('DELETE', deleteRoute('Qwen3-27B')),
    'nothing sent',
  ).toHaveLength(0)
  expect(queryConfirm(main, 'Qwen3-27B'), 'nothing opened').toBeNull()
  expect(confirmRow(main), 'and no confirm row').toBeNull()
  // It follows the switch: switching Qwen3-27B off makes its − active.
  press(switchOf(main, 'Qwen3-27B'))
  await within(main).findByText(en('model-profiles-switched-off', 'Qwen3-27B'))
  expect(
    inactive(minusOf(main, 'Qwen3-27B')),
    '22 D4.2: switched off, its − can act',
  ).toBe(false)
  // And back: switching it on disables it again, with the reason.
  press(switchOf(main, 'Qwen3-27B'))
  await within(main).findByText(en('model-profiles-switched-on', 'Qwen3-27B'))
  const back = minusOf(main, 'Qwen3-27B')
  expect(inactive(back), 'switched on, its − cannot').toBe(true)
  expect(
    text(document.getElementById(back.getAttribute('aria-describedby')!)),
    'and the reason is there',
  ).toBe(en('model-profiles-delete-needs-off'))
})

// ── 22 D4.3, D4.13: the reason beside the − ─────────────────────────────

test('22 D4.3, D4.13, D4.18 (as 22 D4.16 to D4.18 amend them): hovering a disabled −, and focusing it, shows its reason as a .hint-tip box floating over the table (a --surface fill, a --control border, at most min(16rem, 100vw - 16px) wide, wrapping, never over the button, placed by 22 D4.17); it stays while the pointer is over it, the pointer leaving the box hides it, Escape hides it and keeps the focus on the −, a tap on the inactive − shows it and a pointerdown outside the box hides it, and an active − shows nothing', async () => {
  const { main } = await open()
  const box = boxOf(main, 'Qwen3-27B')
  const minus = minusOf(main, 'Qwen3-27B')
  // The reason the − names as its description (22 D4.2), kept hidden.
  const said = document.getElementById(minus.getAttribute('aria-describedby')!)
  expect(said, '22 D4.2: the − names its reason').not.toBeNull()
  // The reason the eye reads: a line of the box, not the hidden description.
  const shown = () =>
    Array.from(box.querySelectorAll('span')).filter(
      (s) => !isHidden(s) && text(s) === en('model-profiles-delete-needs-off'),
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
  expect(
    norm(look['white-space'] ?? 'normal'),
    'and it wraps: no nowrap',
  ).not.toBe('nowrap')
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
  const active = minusOf(main, 'Old-Coder')
  fireEvent.mouseOver(active)
  active.focus()
  await flushTasks()
  expect(
    boxOf(main, 'Old-Coder').children,
    '22 D4.3: an active − shows nothing beside it',
  ).toHaveLength(1)
})

// ── 22 D4.13, D4.16: the reason floats (22D4 review D1, amended) ────────

test('22 D4.13, D4.16 (22 D4 review D1, as the 22D4 amendment amends it): the − and its reason are siblings in the .hint-box of the cell, the − first, and the rule that styles the reason places it against the viewport (position: fixed), so showing it changes no layout', async () => {
  const { main } = await open()
  const cells = cellsOf(rowOf(main, 'Qwen3-27B'))
  const cell = cells[cells.length - 1]!
  const box = boxOf(main, 'Qwen3-27B')
  const minus = minusOf(main, 'Qwen3-27B')
  expect(box.parentElement, 'the box sits in the cell, where the − sat').toBe(
    cell,
  )
  fireEvent.mouseOver(minus)
  await flushTasks()
  const line = Array.from(box.querySelectorAll('span')).find(
    (s) => !isHidden(s) && text(s) === en('model-profiles-delete-needs-off'),
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

// ── 22 D5: the list reason wraps (22D4 D1 decision review) ──────────────

test('22 D5: the list reason wraps (white-space: normal)', () => {
  // 22 D4.15 (WCAG 1.4.10): the status cell of the Models list (index.css
  // 795-797, `.model-profiles td:last-child`) and the Used by cell of the
  // server list (816-819) are `white-space: nowrap`, and the reason inherits
  // that. It stands on one line up to its 16rem, wider than the cell, so at
  // 320 px and at 200 % text it runs past the table's box and the page scrolls
  // sideways. The rule that styles the reason undoes the inherited break, the
  // way `.confirm-row td` does for the confirm question.
  const decls = TOP.filter((r) =>
    r.compact.split(',').includes('.hint-tip'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(decls.length, 'the reason has rules in the sheet').toBeGreaterThan(0)
  expect(
    decls.filter((d) => d.startsWith('white-space:')),
    '22 D5: the rule that styles the reason says how to break its text',
  ).toEqual(['white-space: normal'])
})

// ── 22 D6: the scroll box clips the hidden names (22D4 D5 re-check) ─────

test('22 D6: the table’s scroll box clips the hidden names (position: relative)', () => {
  // 22 D4.8 (WCAG 1.4.10): every row’s − is named by a `.visually-hidden`
  // span, which is `position: absolute` (index.css, `.visually-hidden`). An
  // absolutely positioned box is placed against, and clipped by, the nearest
  // positioned ancestor: `.table-scroll` is not positioned, so the span is
  // placed against the page, sits at the wide table’s right edge and widens
  // the document. The page then scrolls sideways at 320 px and at 200 % text,
  // on both lists, with or without a reason showing (the designer’s
  // re-check of 22 D5, fe/briefs/22-model-sources.md). The scroll box becomes
  // the containing block, so the names it cannot show are clipped inside it.
  const decls = TOP.filter((r) =>
    r.compact.split(',').includes('.table-scroll'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(decls.length, 'the scroll box has rules in the sheet').toBeGreaterThan(
    0,
  )
  expect(
    decls.filter((d) => d.startsWith('position:')),
    '22 D6: the scroll box is positioned, so it is the box the hidden names sit in and are clipped by (1.4.10)',
  ).toEqual(['position: relative'])
})

// ── 22 D7: the reason is not contained (22D4 D7 re-check) ───────────────

test('22 D7: the reason is not contained (no contain on .hint-tip)', () => {
  // 22 D4.15 (WCAG 1.4.10, the designer's re-check of the D6 fix, measured in
  // Chromium, Firefox and WebKit at 320 px, fe/briefs/22-model-sources.md
  // "22D4 D7 re-check"): `contain: inline-size` on the reason takes its inline
  // size out of the layout, so the flex column of `.hint-box` gives it none:
  // the reason collapses to its padding, its text breaks one letter per line,
  // and the pointer cannot stay on it. The reason declares no `contain` at
  // all: its width is its own (22 D4.19's max), it floats (22 D4.16, pinned
  // above) and the box shrank to the button (22 D4.24 took 22 D8's
  // `min-width: 7rem` off, so the box holds no column width: 22 D8 is retired
  // here, its absence pinned by 22 D4.24 in ListColumns.test.tsx).
  const decls = TOP.filter((r) =>
    r.compact.split(',').includes('.hint-tip'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(decls.length, 'the reason has rules in the sheet').toBeGreaterThan(0)
  expect(
    decls.filter((d) => d.startsWith('contain:')),
    '22 D7: no rule that styles the reason declares contain (the D8 fix drops it)',
  ).toEqual([])
})

// ── 22 D4 review D2: the disabled − is drawn inactive ───────────────────

test('22 D4 review D2: the disabled − is drawn inactive the way the aria-disabled buttons are: the sheet gives it the --muted icon on the --bg fill with the default cursor, the rule that draws .primary and .secondary inactive naming .ot-add too, and the active − keeps its --text icon', async () => {
  const { main } = await open()
  const off = minusOf(main, 'Qwen3-27B')
  expect(inactive(off), 'a switched-on row: the − is aria-disabled').toBe(true)
  const box = cascade(off).decls
  expect(norm(box['color'] ?? ''), 'the icon is --muted').toBe('var(--muted)')
  expect(norm(box['background'] ?? ''), 'on the --bg fill').toBe('var(--bg)')
  expect(norm(box['cursor'] ?? ''), 'and the cursor is default').toBe('default')
  const on = minusOf(main, 'Old-Coder')
  expect(inactive(on), 'a switched-off row: the − is active').toBe(false)
  expect(
    norm(cascade(on).decls['color'] ?? ''),
    '22 D4 review D2: the active − keeps its --text icon',
  ).toBe('var(--text)')
})

// ── 22 D4.4: the confirm row ────────────────────────────────────────────

test('22 D4.4: pressing an active − inserts right after the row a tr.confirm-row with one td spanning the three columns, holding the group named by its model-profiles-delete-question paragraph, confirm-delete then confirm-cancel, an empty status line and no alert; the − takes aria-expanded true and the focus goes to Cancel; Cancel and Escape remove the row and focus the −; another row’s confirm closes the first, and the filter hiding the row closes it', async () => {
  const { bff, main } = await open()
  const row = rowOf(main, 'Old-Coder')
  const columns = cellsOf(row).length
  const minus = minusOf(main, 'Old-Coder')
  const group = await confirmGroup(main, 'Old-Coder')
  const tr = confirmRow(main)
  expect(tr, '22 D4.4: the confirm row').not.toBeNull()
  expect(tr!.previousElementSibling, 'right after the profile’s row').toBe(row)
  expect(tr!.parentElement, 'in the table’s body').toBe(row.parentElement)
  const tds = [...tr!.querySelectorAll('td')]
  expect(tds, 'one td').toHaveLength(1)
  expect(Number(tds[0]!.getAttribute('colspan')), 'spanning every column').toBe(
    columns,
  )
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  expect(tds[0]!.contains(group), 'the group is in the td').toBe(true)
  const label = group.getAttribute('aria-labelledby')
  expect(label, 'named by an element').toBeTruthy()
  const asked = document.getElementById(label!)
  expect(asked, 'its paragraph').not.toBeNull()
  expect(asked!.tagName, 'a paragraph').toBe('P')
  expect(text(asked), 'the list’s own question').toBe(question('Old-Coder'))
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
    bff.to('DELETE', deleteRoute('Old-Coder')),
    'nothing sent',
  ).toHaveLength(0)
  press(cancel)
  await flushTasks()
  expect(confirmRow(main), 'Cancel removes the row').toBeNull()
  expect(queryConfirm(main, 'Old-Coder'), 'and the group').toBeNull()
  expect(document.activeElement, 'the focus is back on the −').toBe(minus)
  expect(minus.getAttribute('aria-expanded'), 'closed').toBe('false')
  // Escape.
  const again = await confirmGroup(main, 'Old-Coder')
  expect(document.activeElement, 'open again, on Cancel').toBe(
    confirmIn(again, 'confirm-cancel'),
  )
  fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' })
  await flushTasks()
  expect(confirmRow(main), 'Escape closes it').toBeNull()
  expect(document.activeElement, 'on the −').toBe(minus)
  // One at a time: the list has one switched-off profile, so switch another
  // off and open its confirm while the first is open.
  press(switchOf(main, 'Qwen3-27B'))
  await within(main).findByText(en('model-profiles-switched-off', 'Qwen3-27B'))
  const first = await confirmGroup(main, 'Old-Coder')
  expect(
    confirmRow(main)!.previousElementSibling,
    'the first row’s confirm',
  ).toBe(rowOf(main, 'Old-Coder'))
  const second = await confirmGroup(main, 'Qwen3-27B')
  expect(confirmRow(main), 'one confirm row').not.toBeNull()
  expect(
    confirmRow(main)!.previousElementSibling,
    '22 D4.4: it is the second row’s now',
  ).toBe(rowOf(main, 'Qwen3-27B'))
  expect(
    second.isConnected && first.isConnected,
    'the first group is gone',
  ).not.toBe(true)
  expect(
    minusOf(main, 'Old-Coder').getAttribute('aria-expanded'),
    'and its − says closed',
  ).toBe('false')
  expect(document.activeElement, 'the focus is on the new Cancel').toBe(
    confirmIn(second, 'confirm-cancel'),
  )
  // The filter hiding the row closes it.
  type(filterField(main), 'Coder-30')
  await flushTasks()
  expect(
    dataRows(main).map(nameOf),
    'the filter hides Qwen3-27B',
  ).not.toContain('Qwen3-27B')
  expect(confirmRow(main), '22 D4.4: and its confirm went').toBeNull()
})

// ── 22 D4.5: one delete at a time ───────────────────────────────────────

test('22 D4.5: while the delete runs the group’s status reads confirm-deleting with its buttons aria-disabled, every row’s − and switch are aria-disabled, and a second press sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await open({
    modelProfileDelete: () => gate.promise,
  })
  const group = await confirmGroup(main, 'Old-Coder')
  press(confirmIn(group, 'confirm-delete'))
  await within(group).findByText(en('confirm-deleting'))
  expect(
    bff.to('DELETE', deleteRoute('Old-Coder')),
    'the verb went once',
  ).toHaveLength(1)
  for (const key of ['confirm-delete', 'confirm-cancel'])
    expect(inactive(confirmIn(group, key)), `${key} is aria-disabled`).toBe(
      true,
    )
  for (const p of LIST)
    expect(inactive(minusOf(main, p.id)), `${p.id}’s − is aria-disabled`).toBe(
      true,
    )
  for (const s of switches(main))
    expect(inactive(s), 'every switch is aria-disabled').toBe(true)
  press(confirmIn(group, 'confirm-delete'))
  press(minusOf(main, 'Old-Coder'))
  await flushTasks()
  expect(
    bff.to('DELETE', deleteRoute('Old-Coder')),
    '22 D4.5: a second press sends nothing',
  ).toHaveLength(1)
  gate.resolve(reply(204))
  await within(main).findByText(en('model-profiles-deleted', 'Old-Coder'))
})

// ── 22 D4.6: done ───────────────────────────────────────────────────────

test('22 D4.6: a 204 removes the profile’s row and the confirm row, the region line above the table says model-profiles-deleted with the id, the focus goes to the next row’s − and else to the previous row’s, and the list is not read again', async () => {
  const { bff, main } = await open()
  const group = await confirmGroup(main, 'Old-Coder')
  const reads = bff.to('GET', '/api/profiles/model').length
  press(confirmIn(group, 'confirm-delete'))
  const done = await within(main).findByText(
    en('model-profiles-deleted', 'Old-Coder'),
  )
  expect(dataRows(main).map(nameOf), 'the row is gone').toEqual([
    'Qwen3-27B',
    'Coder-30B',
  ])
  expect(confirmRow(main), 'and so is the confirm row').toBeNull()
  expect(done.getAttribute('role'), '22 D4.6: the region’s status line').toBe(
    'status',
  )
  expect(before(done, table(main)), 'above the table').toBe(true)
  expect(done, 'the switch’s own line, not a new one').toBe(regionStatus(main))
  expect(
    bff.to('GET', '/api/profiles/model').length,
    '22 D4.6: the list was not read again',
  ).toBe(reads)
  expect(
    document.activeElement,
    'the last row is gone: the focus is on the previous row’s −',
  ).toBe(minusOf(main, 'Coder-30B'))
  // The next row’s − when there is one.
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/model')
  const second = await open({
    modelProfiles: [
      modelProfile('Old-Coder', false),
      modelProfile('Qwen3-27B', true),
    ],
  })
  const g2 = await confirmGroup(second.main, 'Old-Coder')
  press(confirmIn(g2, 'confirm-delete'))
  await within(second.main).findByText(
    en('model-profiles-deleted', 'Old-Coder'),
  )
  expect(
    document.activeElement,
    '22 D4.6: the focus is on the next row’s −',
  ).toBe(minusOf(second.main, 'Qwen3-27B'))
})

// ── 22 D4.7: refusals ───────────────────────────────────────────────────

test('22 D4.7: a 409 model-profile-enabled reads the list again, the confirm row goes, the row shows the profile switched on with its − disabled and focusable, api-model-profile-enabled is in the region’s alert above the table and the focus is on the row’s −', async () => {
  let reads = 0
  const { bff, main } = await open({
    modelProfiles: () =>
      reply(200, ++reads === 1 ? LIST : LIST.map((p) => ({ ...p }))),
    modelProfileDelete: () =>
      reply(409, { key: 'model-profile-enabled', args: ['Old-Coder'] }),
  })
  const refusal = en('api-model-profile-enabled', 'Old-Coder')
  const group = await confirmGroup(main, 'Old-Coder')
  press(confirmIn(group, 'confirm-delete'))
  await within(main).findByText(refusal)
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(bff.to('GET', '/api/profiles/model').length, 'twice').toBe(2)
  expect(confirmRow(main), 'the confirm row is gone').toBeNull()
  expect(queryConfirm(main, 'Old-Coder'), 'and so is the group').toBeNull()
  const kept = within(main).getByText(refusal)
  expect(kept.getAttribute('role'), 'the region’s alert').toBe('alert')
  expect(before(kept, table(main)), 'above the table').toBe(true)
  expect(kept, 'the region’s own alert line').toBe(regionAlert(main))
  const minus = minusOf(main, 'Old-Coder')
  expect(
    switchOf(main, 'Old-Coder').checked,
    'the row shows the profile switched on',
  ).toBe(true)
  expect(inactive(minus), 'with its − disabled').toBe(true)
  expect(document.activeElement, '22 D4.7: the focus is on the −').toBe(minus)
  expect(minus, 'and it is still focusable, not gone').not.toBe(document.body)
})

test('22 D4.7: a 409 model-profile-assigned puts api-model-profile-assigned in the group’s alert with the focus staying on confirm-delete, the row staying and the list not read again', async () => {
  const { bff, main } = await open({
    modelProfileDelete: () =>
      reply(409, {
        key: 'model-profile-assigned',
        args: ['Old-Coder', 'coder'],
      }),
  })
  const refusal = en('api-model-profile-assigned', 'Old-Coder', 'coder')
  const group = await confirmGroup(main, 'Old-Coder')
  const verb = confirmIn(group, 'confirm-delete')
  press(verb)
  const alert = await within(group).findByText(refusal)
  expect(alert.closest('[role="alert"]'), 'in the group’s alert').not.toBeNull()
  expect(group.contains(alert), 'the alert is the group’s').toBe(true)
  expect(document.activeElement, 'the focus stays on confirm-delete').toBe(verb)
  expect(confirmRow(main), 'the confirm row stays').not.toBeNull()
  expect(
    bff.to('GET', '/api/profiles/model').length,
    'nothing is read again',
  ).toBe(1)
})

test('22 D4.7: a 404 no-model-profile-named reads the list again, puts api-no-model-profile-named in the region’s alert above the table, and moves the focus as after a 204', async () => {
  let reads = 0
  const { main } = await open({
    modelProfiles: () =>
      reply(
        200,
        ++reads === 1
          ? LIST
          : LIST.filter((p) => p.id !== 'Old-Coder').map((p) => ({ ...p })),
      ),
    modelProfileDelete: () =>
      reply(404, { key: 'no-model-profile-named', args: ['Old-Coder'] }),
  })
  const refusal = en('api-no-model-profile-named', 'Old-Coder')
  const group = await confirmGroup(main, 'Old-Coder')
  press(confirmIn(group, 'confirm-delete'))
  const kept = await within(main).findByText(refusal)
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(kept.getAttribute('role'), 'the region’s alert').toBe('alert')
  expect(before(kept, table(main)), 'above the table').toBe(true)
  expect(confirmRow(main), 'the confirm row is gone').toBeNull()
  expect(dataRows(main).map(nameOf), 'the row is gone with the file').toEqual([
    'Qwen3-27B',
    'Coder-30B',
  ])
  expect(
    document.activeElement,
    '22 D4.7: the focus moved as after a 204',
  ).toBe(minusOf(main, 'Coder-30B'))
})

test('22 D4.7 (22 D4 review D4): opening a confirm clears the region’s alert: after a 404 leaves api-no-model-profile-named above the table, the next row’s confirm opens with no alert, and its successful delete leaves the was-deleted line alone', async () => {
  let reads = 0
  const { main } = await open({
    modelProfiles: () =>
      reply(
        200,
        ++reads === 1
          ? LIST
          : [
              modelProfile('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
              modelProfile('Coder-30B', false),
            ],
      ),
    modelProfileDelete: (id) =>
      id === 'Old-Coder'
        ? reply(404, { key: 'no-model-profile-named', args: ['Old-Coder'] })
        : reply(204),
  })
  const group = await confirmGroup(main, 'Old-Coder')
  press(confirmIn(group, 'confirm-delete'))
  await within(main).findByText(en('api-no-model-profile-named', 'Old-Coder'))
  await flushTasks()
  expect(
    alertTexts(main),
    'the 404 leaves its alert above the table',
  ).toHaveLength(1)
  // The next delete of the same region: opening its confirm clears the old
  // alert, so the was-deleted line of the success stands alone (D4).
  const next = await confirmGroup(main, 'Coder-30B')
  expect(confirmRow(main), 'the confirm row is open').not.toBeNull()
  expect(
    alertTexts(main),
    '22 D4 review D4: opening a confirm clears the region’s alert',
  ).toEqual([])
  press(confirmIn(next, 'confirm-delete'))
  const done = await within(main).findByText(
    en('model-profiles-deleted', 'Coder-30B'),
  )
  expect(done.getAttribute('role'), 'the region’s line says it').toBe('status')
  expect(alertTexts(main), 'and no old alert sits beside it').toEqual([])
})

test('22 D4.7: a 504 client-timeout shows delete-unknown in the group, then reads the list again: the row gone, the same text moves to the region’s alert and the focus moves as after a 204; the row still there, the group keeps it and the focus stays on confirm-delete', async () => {
  const timeout = () => reply(504, { key: 'client-timeout', args: [] })
  // The row is gone.
  let reads = 0
  const gone = await open({
    modelProfiles: () =>
      reply(
        200,
        ++reads === 1
          ? LIST
          : LIST.filter((p) => p.id !== 'Old-Coder').map((p) => ({ ...p })),
      ),
    modelProfileDelete: timeout,
  })
  const g = await confirmGroup(gone.main, 'Old-Coder')
  press(confirmIn(g, 'confirm-delete'))
  const said = await within(gone.main).findByText(en('delete-unknown'))
  expect(
    said.closest('[role="alert"]'),
    '504: delete-unknown, never request-failed',
  ).not.toBeNull()
  expect(g.contains(said), 'first in the group').toBe(true)
  await within(gone.main).findAllByText(en('delete-unknown'))
  await flushTasks()
  expect(reads, 'the list was read again').toBe(2)
  expect(dataRows(gone.main).map(nameOf), 'the row is gone').toEqual([
    'Qwen3-27B',
    'Coder-30B',
  ])
  expect(confirmRow(gone.main), 'the confirm row went with it').toBeNull()
  const moved = within(gone.main).getByText(en('delete-unknown'))
  expect(moved.getAttribute('role'), 'it moved to the region’s alert').toBe(
    'alert',
  )
  expect(before(moved, table(gone.main)), 'above the table').toBe(true)
  expect(document.activeElement, 'and the focus moved as after a 204').toBe(
    minusOf(gone.main, 'Coder-30B'),
  )
  // The row is still there.
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/model')
  const kept = await open({ modelProfileDelete: timeout })
  const g2 = await confirmGroup(kept.main, 'Old-Coder')
  const verb = confirmIn(g2, 'confirm-delete')
  press(verb)
  await within(g2).findByText(en('delete-unknown'))
  await flushTasks()
  expect(dataRows(kept.main).map(nameOf), 'the row is still there').toContain(
    'Old-Coder',
  )
  expect(confirmRow(kept.main), 'the confirm row stays').not.toBeNull()
  expect(groupStatus(g2).textContent, 'and its status line is empty').toBe('')
  expect(document.activeElement, 'the focus stays on confirm-delete').toBe(verb)
})

test('22 D4.7: a 401 shows session-ended-retry with a Sign in in the group; a 503, a 403 session-not-administrator, a 422 with a key the list does not know and a 500 each show their text in the group’s alert with the focus on confirm-delete, the row staying and the list not read again', async () => {
  const cases: Array<[string, () => Response, string]> = [
    [
      '503 client-unavailable',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '403 session-not-administrator',
      () => reply(403, { key: 'session-not-administrator', args: [] }),
      en('session-revoked'),
    ],
    [
      '422 with a key the list does not know',
      () =>
        reply(422, {
          key: 'model-profile-bad-source',
          field: 'values',
          args: ['Old-Coder'],
        }),
      en('api-other', 'model-profile-bad-source'),
    ],
    [
      '500',
      () => reply(500, { key: 'client-write-failed', args: [] }),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, words] of cases) {
    cleanup()
    vi.unstubAllGlobals()
    startAt('#/profiles/model')
    const { bff, main } = await open({ modelProfileDelete: answer })
    const group = await confirmGroup(main, 'Old-Coder')
    const verb = confirmIn(group, 'confirm-delete')
    press(verb)
    const alert = await within(group).findByText(words)
    expect(
      alert.closest('[role="alert"]'),
      `${what}: the text is in a role="alert"`,
    ).not.toBeNull()
    expect(group.contains(alert), `${what}: the alert is the group’s`).toBe(
      true,
    )
    expect(document.activeElement, `${what}: the focus stays on the verb`).toBe(
      verb,
    )
    expect(dataRows(main).map(nameOf), `${what}: the row stays`).toContain(
      'Old-Coder',
    )
    expect(
      bff.to('GET', '/api/profiles/model').length,
      `${what}: nothing read again`,
    ).toBe(1)
  }
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/profiles/model')
  const ended = await open({
    modelProfileDelete: () => reply(401, { key: 'session-required', args: [] }),
  })
  const group = await confirmGroup(ended.main, 'Old-Coder')
  press(confirmIn(group, 'confirm-delete'))
  const line = await within(group).findByText(en('session-ended-retry'))
  expect(line.closest('[role="alert"]'), 'the group’s alert').not.toBeNull()
  const signIn = within(group).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(
    line.compareDocumentPosition(signIn) & Node.DOCUMENT_POSITION_FOLLOWING,
    '22 D4.7: a Sign in after the words',
  ).not.toBe(0)
})

// ── 22 D4.8: widths ─────────────────────────────────────────────────────

test('22 D4.8: the confirm row is a row of the table, so its group sits inside the table’s scroll box and wraps with it: the td declares no width of its own and the group no width, and the cell keeps its 40 px target', async () => {
  const { main } = await open()
  const group = await confirmGroup(main, 'Old-Coder')
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
    norm(
      cascade(cellsOf(rowOf(main, 'Old-Coder')).slice(-1)[0] as HTMLElement)
        .decls['flex-wrap'] ?? '',
    ),
    '22 D4.25: the − has its own cell, which is no flex box and wraps nothing: the 40 px target is the button’s own, and the confirm row spans the row’s columns',
  ).toBe('')
})
