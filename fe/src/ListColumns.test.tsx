// Brief 22, the "22D4 composition: a column for the row's actions" amendment
// (fe/briefs/22-model-sources.md, 22 D4.23 to D4.28 and D4.34): both profile
// lists gain a last column, Actions, whose header is visually hidden and whose
// one cell holds the row's trash can; the cells stop being flex boxes so every
// control sits centred in its row; the status and actions columns shrink to
// their content and the spare width goes to the text columns; and the server
// list's Used by cell is one link of the instance's name, the id as muted plain
// text after it.
//
// The behaviour the amendment leaves alone is pinned where it was: the floating
// tip and the round button in ListDeleteTip.test.tsx, the delete's answers in
// ModelProfilesDelete1.test.tsx and ServerProfilesDelete1.test.tsx, the columns
// of each list in ModelProfiles.test.tsx and ServerProfiles1.test.tsx.
//
// D4.28 splits the pins: what is written here is the structure and the sheet
// text. The centring (every control's centre within 1 px of its row's), the
// trash can's edge being the padding from the row's end, the two columns being
// no wider than their content, and the absence of a sideways page scroll at
// 320 px are measurements: jsdom lays nothing out, so the designer measures them
// in the three browsers against a screenshot (fe/ux/check.js). The helpers and
// the stylesheet reader are local copies: a test file imports no other test
// file.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  en,
  fakeBff,
  reply,
  serverProfileDoc,
  shownText,
  startAt,
  type BffOptions,
  type Instance,
  type ModelProfile,
  type ServerProfile,
} from './test/bff.ts'

// ── The two lists ───────────────────────────────────────────────────────

function modelProfile(id: string, enabled: boolean): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model: '' }
}

/** One row switched on, one switched off: only the off row's delete can act. */
const MODEL_LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true),
  modelProfile('Old-Coder', false),
]

function serverRow(id: string, instance: string | null): ServerProfile {
  return { id, engine: 'llama.cpp', port: 8580, tags: [], instance }
}

/** One profile an instance runs, one free: only the free row can act. */
const SERVER_ROWS: ServerProfile[] = [
  serverRow('default', 'default'),
  serverRow('qwen-27b', null),
]

/** The instance of 22 D4.34: a label, so its name is not its id and the id
 *  line is there to tell two instances named alike apart (16 17f.4). */
const LABELLED: Instance[] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    state: 'active',
    label: 'Coding assistant',
  },
]

/** One of the two lists whose row delete is the shared `RowDelete`. */
type List = {
  label: string
  /** The table's accessible name. */
  titleKey: string
  /** The page, with its table read. */
  open: (options?: BffOptions) => Promise<HTMLElement>
  /** The row delete's name, with the row's id (22 D4.1). */
  deleteKey: string
  /** The header of the column that held the delete before 22 D4.23. */
  valueKey: string
  /** The columns, in order, with the new Actions last (22 D4.23, and the
   *  user's condition of 2026-11-25: Used by keeps its own column). */
  columns: string[]
  /** A row whose delete cannot act, and one whose can. */
  blocked: string
  free: string
}

const MODELS: List = {
  label: 'the Models list',
  titleKey: 'model-profiles-title',
  open: async (options = {}) => {
    startAt('#/profiles/model')
    fakeBff({ modelProfiles: MODEL_LIST, ...options })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('table', { name: en('model-profiles-title') })
    await flushTasks()
    return main
  },
  deleteKey: 'model-profiles-delete-row',
  valueKey: 'model-profiles-column-status',
  columns: [
    'model-profiles-column-name',
    'model-profiles-column-engine',
    'model-profiles-column-status',
    'profiles-column-actions',
  ],
  blocked: 'Qwen3-27B',
  free: 'Old-Coder',
}

const SERVERS: List = {
  label: 'the Server profiles list',
  titleKey: 'server-profiles-title',
  open: async (options = {}) => {
    startAt('#/profiles/server')
    fakeBff({ serverProfiles: SERVER_ROWS, ...options })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('table', {
      name: en('server-profiles-title'),
    })
    await flushTasks()
    return main
  },
  deleteKey: 'server-profiles-delete-row',
  valueKey: 'server-profiles-column-used-by',
  columns: [
    'server-profiles-column-name',
    'server-profiles-column-port',
    'server-profiles-column-tags',
    'server-profiles-column-used-by',
    'profiles-column-actions',
  ],
  blocked: 'default',
  free: 'qwen-27b',
}

const LISTS = [MODELS, SERVERS]

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a promise chain) run. */
async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

/** The words a node shows, whitespace folded. */
function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function tableOf(main: HTMLElement, list: List): HTMLElement {
  return within(main).getByRole('table', { name: en(list.titleKey) })
}

/** The rows that carry data, in the reply order. */
function dataRows(t: HTMLElement): HTMLElement[] {
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

/** The headers, in the sheet's order. */
function headersOf(t: HTMLElement): HTMLElement[] {
  return within(t).getAllByRole('columnheader')
}

/** The row of `id`, by its first cell. */
function rowOf(main: HTMLElement, list: List, id: string): HTMLElement {
  const row = dataRows(tableOf(main, list)).find((r) =>
    (r.querySelector('td')?.textContent ?? '').includes(id),
  )
  expect(row, `${list.label}: the row of ${id}`).not.toBeUndefined()
  return row!
}

/** The cells of a row. */
function cellsOf(row: Element): HTMLElement[] {
  return [...row.querySelectorAll('td')]
}

/** The index of the column whose header reads `key`. */
function columnOf(t: HTMLElement, key: string): number {
  const i = [...t.querySelectorAll('thead th')].findIndex(
    (th) => text(th) === en(key),
  )
  expect(i, `${en(key)} is a column`).toBeGreaterThan(-1)
  return i
}

/** The row's delete, named with the row's id (22 D4.1). */
function deleteOf(row: HTMLElement, list: List, id: string): HTMLElement {
  return within(row).getByRole('button', { name: en(list.deleteKey, id) })
}

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── The stylesheet reader (the local copy of 22 D4.1) ───────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = { selector: string; body: string; at: number; depth: number }

const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  selector: (m[1] ?? '').replace(/\s+/g, ' ').trim(),
  body: m[2] ?? '',
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

const TOP = RULES.filter((r) => r.depth === 0)

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

/** Every declaration of every top-level rule whose selector list names `sel`
 *  exactly, in the sheet's order. */
function declsOf(sel: string): string[] {
  return TOP.filter((r) =>
    r.selector
      .split(',')
      .map((p) => norm(p))
      .includes(sel),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
}

/** The declarations of a property, in the sheet's order. */
function values(decls: string[], prop: string): string[] {
  return decls
    .filter((d) => d.startsWith(`${prop}:`))
    .map((d) => norm(d.slice(prop.length + 1)))
}

/** A selector token split into its base and its pseudo-classes and attribute
 *  conditions, which are read one at a time. */
function splitToken(t: string): { base: string; pseudo: string[] } {
  const i = t.search(/[:[]/)
  if (i < 0) return { base: t, pseudo: [] }
  return {
    base: t.slice(0, i),
    pseudo: t.slice(i).match(/\[[^\]]*\]|:[a-z-]+(\([^)]*\))?/g) ?? [],
  }
}

/** The simple selectors of a base: `td`, `.ot-add.ot-delete` as two. */
function simple(base: string): string[] {
  return base.match(/(#[^.#]+|\.[^.#]+|^[a-zA-Z][-\w]*)/g) ?? []
}

/** The simple selectors an element answers: its tag, its classes, its id. */
function own(el: Element): Set<string> {
  const s = new Set<string>([el.tagName.toLowerCase()])
  for (const c of el.classList) s.add(`.${c}`)
  if (el.id) s.add(`#${el.id}`)
  return s
}

/** The same for every ancestor, for a descendant token (`.model-profiles td`). */
function ancestors(el: Element): Set<string> {
  const s = new Set<string>()
  for (let p = el.parentElement; p; p = p.parentElement) {
    s.add(p.tagName.toLowerCase())
    for (const c of p.classList) s.add(`.${c}`)
    if (p.id) s.add(`#${p.id}`)
  }
  return s
}

/** A pseudo-class of the last token, answered. A state a test cannot reach in
 *  jsdom (a hover, a focus) answers false, so a hover rule never lends its
 *  value to a look read off the page. */
function pseudoMatches(el: HTMLElement, p: string): boolean {
  if (p === ':hover' || p === ':focus' || p.startsWith(':focus-')) return false
  if (p === ':last-child') return el.parentElement?.lastElementChild === el
  if (p === ':first-child') return el.parentElement?.firstElementChild === el
  const nth = /^:nth-child\((\d+)\)$/.exec(p)
  if (nth !== null) {
    const kids = Array.from(el.parentElement?.children ?? [])
    return kids.indexOf(el) + 1 === Number(nth[1])
  }
  if (p.startsWith(':not(')) return !el.matches(p.slice(5, -1))
  if (p.startsWith('[')) return el.matches(p)
  return false
}

/** One selector part: its last token is the element (its tag, a class, its id),
 *  its earlier tokens are classes of the element's ancestors. The combinator is
 *  read as a descendant, which is all these lists' rules use. */
function partMatches(el: HTMLElement, part: string): boolean {
  const ts = part.split(/\s+/).filter((t) => t !== '' && t !== '>')
  if (ts.length === 0) return false
  const mine = own(el)
  const above = ancestors(el)
  for (let i = 0; i < ts.length; i++) {
    const last = i === ts.length - 1
    const { base, pseudo } = splitToken(ts[i]!)
    const set = last ? mine : above
    for (const s of simple(base)) if (!set.has(s)) return false
    if (last) for (const p of pseudo) if (!pseudoMatches(el, p)) return false
  }
  return true
}

/** The declarations that reach `el`, merged in the cascade's order: the local
 *  copy of the reader of 22 D2.2, reading a scoped rule (`.model-profiles td`)
 *  by its last token against the element and its earlier tokens against its
 *  ancestors. */
function cascade(el: HTMLElement): Record<string, string> {
  const hits = TOP.filter((r) =>
    r.selector.split(',').some((p) => partMatches(el, p)),
  )
  const spec = (r: Rule): number =>
    Math.max(
      ...r.selector.split(',').map((p) => (p.match(/[.#]/g) ?? []).length),
    )
  hits.sort((a, b) => spec(a) - spec(b) || a.at - b.at)
  const out: Record<string, string> = {}
  for (const r of hits)
    for (const d of r.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[norm(d.slice(0, i))] = norm(d.slice(i + 1))
    }
  return out
}

/** Every top-level rule whose selector ends in `td` or `.status-cell`: the
 *  rules that style a table cell directly. */
function cellRules(): Rule[] {
  return TOP.filter((r) =>
    r.selector.split(',').some((p) => {
      const ts = p.split(/\s+/).filter((t) => t !== '' && t !== '>')
      const last = ts[ts.length - 1]
      return last !== undefined && simple(splitToken(last).base).includes('td')
    }),
  ).concat(
    TOP.filter((r) =>
      r.selector.split(',').some((p) => {
        const ts = p.split(/\s+/).filter((t) => t !== '' && t !== '>')
        const last = ts[ts.length - 1]
        return (
          last !== undefined &&
          simple(splitToken(last).base).includes('.status-cell')
        )
      }),
    ),
  )
}

// ── 22 D4.23: a column for the row's actions ────────────────────────────

test('22 D4.23: both lists end with one column more, a th scope="col" holding only a span.visually-hidden with profiles-column-actions, in the order the user fixed (Used by keeps its own column), and the sheet hides .visually-hidden', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    expect(
      headers.map((h) => text(h)),
      `${list.label}: the columns, in order, Actions last`,
    ).toEqual(list.columns.map((k) => en(k)))
    const last = headers[headers.length - 1]!
    expect(
      last.getAttribute('scope'),
      `${list.label}: the new header names its column`,
    ).toBe('col')
    const kids = [...last.children]
    expect(kids, `${list.label}: the th holds one element`).toHaveLength(1)
    expect(kids[0]!.tagName, `${list.label}: a span`).toBe('SPAN')
    expect(
      kids[0]!.classList.contains('visually-hidden'),
      `${list.label}: the word is in the DOM and not shown, so a screen reader announces the column and the eye sees no heading over an icon`,
    ).toBe(true)
    expect(
      within(t).getAllByRole('columnheader', {
        name: en('profiles-column-actions'),
      }),
      `${list.label}: the column is announced once`,
    ).toHaveLength(1)
    // The sheet is what makes `.visually-hidden` not shown (the pair of the
    // class link above, 22 D4.28).
    expect(
      values(declsOf('.visually-hidden'), 'position'),
      'the hidden word is out of the flow',
    ).toContain('absolute')
    cleanup()
  }
})

test('22 D4.23: every row of both lists has its trash can in the Actions cell, the Status cell (Models) and the Used by cell (Server profiles) hold no button and keep their value, and every row has one cell under every header', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    const value = columnOf(t, list.valueKey)
    for (const row of dataRows(t)) {
      const cells = cellsOf(row)
      expect(cells, `${list.label}: one cell under every header`).toHaveLength(
        headers.length,
      )
      const id = text(cells[0])
      const del = deleteOf(row, list, id)
      expect(
        del.closest('td'),
        `${list.label}: the row's delete is in the last cell`,
      ).toBe(cells[cells.length - 1])
      expect(
        cells[value]!.querySelectorAll('button').length,
        `${list.label}: the ${en(list.valueKey)} cell holds no delete`,
      ).toBe(0)
    }
    if (list === MODELS) {
      // The Status cell keeps the switch and its word (22 D4.23).
      for (const row of dataRows(t)) {
        const cell = cellsOf(row)[columnOf(t, list.valueKey)]!
        expect(
          cell.querySelectorAll('input'),
          'the Status cell keeps the switch',
        ).toHaveLength(1)
        const p = MODEL_LIST.find(
          (m) => m.id === text(cellsOf(row)[0]),
        )!.enabled
        expect(text(cell), 'and the word that says the state').toBe(
          en(p ? 'model-profiles-status-on' : 'model-profiles-status-off'),
        )
      }
    } else {
      // The Used by cell keeps the instance link, or the word for a free
      // profile (22 D4.23).
      const used = cellsOf(rowOf(main, list, list.blocked))[
        columnOf(t, list.valueKey)
      ]!
      expect(
        within(used).getByRole('link').getAttribute('href'),
        'the in-use row keeps the link to its instance',
      ).toBe('#/instances/default')
      const free = cellsOf(rowOf(main, list, list.free))[
        columnOf(t, list.valueKey)
      ]!
      expect(text(free), 'the free row keeps the word').toBe(
        en('server-profiles-unused'),
      )
    }
    cleanup()
  }
})

test('22 D4.23: the confirm row of both lists spans the new column count (a day-one guard: today three and four equal the three and four headers)', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    const row = rowOf(main, list, list.free)
    fireEvent.click(deleteOf(row, list, list.free))
    const tr = main.querySelector('tr.confirm-row')
    expect(tr, `${list.label}: the confirm row`).not.toBeNull()
    expect(tr!.previousElementSibling, 'right after the row').toBe(row)
    const tds = [...tr!.querySelectorAll('td')]
    expect(tds, 'one td').toHaveLength(1)
    expect(
      Number(tds[0]!.getAttribute('colspan')),
      `${list.label}: the confirm spans every column`,
    ).toBe(headers.length)
    cleanup()
  }
})

// ── 22 D4.4, the L1 review: the confirm td is the last cell of its row ───

test('22 D4.4 (L1 review D1): the confirm row of both lists asks its question in a td that wraps, is not aligned to the end and takes no 1% width, even though that one td is the last child of its row and so is reached by the Actions column rules of 22 D4.24', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const row = rowOf(main, list, list.free)
    fireEvent.click(deleteOf(row, list, list.free))
    const tr = main.querySelector('tr.confirm-row')
    expect(tr, `${list.label}: the confirm row`).not.toBeNull()
    const tds = [...tr!.querySelectorAll('td')]
    expect(tds, `${list.label}: its one td`).toHaveLength(1)
    const td = tds[0]!
    // The mechanism the review names: the one td IS the row's last child, so
    // `.model-profiles td:last-child` / `.server-profiles td:last-child`
    // (index.css, the Actions column) reach it as they reach a data row's.
    expect(
      tr!.lastElementChild,
      `${list.label}: the confirm td is the row's last child`,
    ).toBe(td)
    const look = cascade(td)
    expect(
      look['white-space'],
      `${list.label}: the question is a sentence, so it wraps (at 320 px it stops wrapping under the nowrap)`,
    ).toBe('normal')
    expect(
      look['text-align'],
      `${list.label}: the question and its buttons are not pushed to the end of the row`,
    ).not.toBe('end')
    expect(
      look['width'],
      `${list.label}: the confirm spans the row, it does not shrink to its content`,
    ).not.toBe('1%')
    // The sheet's own rule for the confirm cell, the pair of the cascade read
    // (22 D4.4: "declares no width of its own and wraps").
    expect(
      values(declsOf('.confirm-row td'), 'white-space'),
      `${list.label}: .confirm-row td declares the wrap`,
    ).toContain('normal')
    cleanup()
  }
})

// ── 22 D4.8, the L1 review: the question stays inside the box ───────────

test('22 D4.8 (L1 review D2): the scroll box is an inline-size container and the confirm group is sticky at its inline start and capped to the box, so at 320 px the question of a confirm row is wholly in view at any scroll position', () => {
  // The mechanism the re-check measured (fe/briefs/22-model-sources.md "L1
  // re-check"): the confirm td spans the whole table, 318-320 px on the models
  // list and 757-759 px on the server list, wider than the 286 px box, so the
  // question is laid out over that width and its start sits out of view once
  // the box is scrolled to the trash can that opened it. The box is the only
  // thing that knows the width the user sees, so it becomes the container the
  // group measures itself against, and the group sticks to its inline start.
  // Both are sheet declarations at the rules' real places, measured in
  // Chromium, Firefox and WebKit; jsdom lays nothing out, so the sheet text is
  // the pin (22 D4.28).
  const box = declsOf('.table-scroll')
  expect(
    box.length,
    'the sheet has a rule whose selector is .table-scroll',
  ).toBeGreaterThan(0)
  expect(
    values(box, 'container-type'),
    "the scroll box is the container: the group's cap is a container query unit, which resolves against the nearest inline-size container",
  ).toContain('inline-size')
  const group = declsOf('.confirm-group')
  expect(
    group.length,
    'the sheet has a rule whose selector is .confirm-group',
  ).toBeGreaterThan(0)
  expect(
    values(group, 'position'),
    'the group sticks inside the box as the table scrolls, so the question stays where the reader is',
  ).toContain('sticky')
  expect(
    values(group, 'left'),
    'stuck to the inline start of the box',
  ).toContain('0')
  expect(
    values(group, 'max-width'),
    'and no wider than the box less its padding, so the question wraps inside it (measured: 270 px in four lines, 1280 px unchanged)',
  ).toContain('calc(100cqi - 2 * var(--space-2))')
})

// ── 22 D4.24: shrink to the content, aligned to the end ─────────────────

test('22 D4.24: the Actions column is as narrow as its button and its cell ends the row, the model Status column shrinks too, and the hint box is as wide as the button', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    const th = headers[headers.length - 1]!
    const td = cellsOf(dataRows(t)[0]!)[headers.length - 1]!
    const head = cascade(th)
    const cell = cascade(td)
    expect(head['width'], `${list.label}: the Actions head shrinks`).toBe('1%')
    expect(
      head['white-space'],
      `${list.label}: the Actions head keeps one line`,
    ).toBe('nowrap')
    expect(
      cell['width'],
      `${list.label}: the Actions cell shrinks to its button`,
    ).toBe('1%')
    expect(
      cell['white-space'],
      `${list.label}: the Actions cell keeps one line`,
    ).toBe('nowrap')
    expect(
      cell['text-align'],
      `${list.label}: the cell is aligned to the end, so the trash can ends the row`,
    ).toBe('end')
    cleanup()
  }
  // The model Status column shrinks with it, so the spare width goes to the
  // name and the engine.
  const main = await MODELS.open()
  const t = tableOf(main, MODELS)
  const status = cellsOf(dataRows(t)[0]!)[columnOf(t, MODELS.valueKey)]!
  expect(cascade(status)['width'], 'the Status column shrinks').toBe('1%')
  expect(
    cascade(status)['white-space'],
    'the Status column keeps one line',
  ).toBe('nowrap')
  cleanup()
  // The box of the delete and its reason is as wide as the button: no
  // min-width, and an inline box (22 D4.24 drops 22 D8's 7rem).
  const box = declsOf('.hint-box')
  expect(
    values(box, 'min-width'),
    'the hint box keeps no room of its own',
  ).toEqual([])
  expect(
    values(box, 'display'),
    'the hint box is as wide as its button',
  ).toContain('inline-flex')
})

// ── 22 D4.25: every control centred in its row ──────────────────────────

test('22 D4.25: the model Status cell wraps its switch and its word in a span inside a plain td, every list cell centres its content, and the row delete carries no top margin while the add after a list keeps its own', async () => {
  const main = await MODELS.open()
  const t = tableOf(main, MODELS)
  const cell = cellsOf(dataRows(t)[0]!)[columnOf(t, MODELS.valueKey)]!
  const input = cell.querySelector('input')!
  expect(input, 'the Status cell holds the switch').not.toBeNull()
  const wrap = input.closest('span')
  expect(
    wrap,
    'the switch and its word are wrapped in a span, not laid out by the cell',
  ).not.toBeNull()
  expect(
    wrap!.parentElement,
    "the span is the cell's own child: the td is a plain table cell",
  ).toBe(cell)
  expect(
    text(wrap!),
    'the span holds the word too, so the two are one line',
  ).toBe(en('model-profiles-status-on'))
  cleanup()
  // Every list cell centres its content: the cell is a table cell again, so
  // vertical-align is what centres it (22 D4.25).
  for (const list of LISTS) {
    const m = await list.open()
    const table = tableOf(m, list)
    for (const row of dataRows(table))
      for (const td of cellsOf(row))
        expect(
          cascade(td)['vertical-align'],
          `${list.label}: ${text(td)}: the cell centres its content`,
        ).toBe('middle')
    cleanup()
  }
  // The row delete has no top margin; the add after a list keeps its room.
  expect(
    values(declsOf('.ot-add.ot-delete'), 'margin-top'),
    'the row delete sits where the cell puts it',
  ).toContain('0')
  expect(
    values(declsOf('.ot-add'), 'margin-top'),
    'the add after a list keeps its --space-3',
  ).toContain('var(--space-3)')
})

// ── 22 D4.25, the L1 review: the switch's line is on the row's middle ────

test('22 D4.25 (L1 review D1): the span that carries the switch and its word is a flex box, not an inline-flex one, so the cell centres it in the row (measured: 1.70 to 1.78 px off the row centre as an inline box, 0.50 to 0.67 px as flex)', () => {
  // The rule the review names (index.css, `.status-cell > span`), read by its
  // exact selector part: an inline box sits on the cell's line box and follows
  // its baseline, so `vertical-align: middle` on the td never brings it to the
  // row's middle. The sheet's rule is the fix, so the sheet text is the pin.
  const box = declsOf('.status-cell > span')
  expect(
    box.length,
    'the sheet has a rule whose selector is .status-cell > span',
  ).toBeGreaterThan(0)
  const display = values(box, 'display')
  expect(
    display,
    'the rule declares display: flex, the shape measured within 1 px',
  ).toContain('flex')
  expect(
    display,
    'nothing inline-flex left in that rule: an inline box is what sits on the text line',
  ).not.toContain('inline-flex')
})

// ── 22 D4.26: no dead space ─────────────────────────────────────────────

test('22 D4.26: the table fills the card and the spare width goes to the text columns: no width on the name, the engine, the tags or the Used by cell, a 1% width on the status and the actions cell', async () => {
  expect(
    values(declsOf('table'), 'width'),
    'the table fills the card',
  ).toContain('100%')
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    const shrink = [MODELS.valueKey, 'profiles-column-actions']
    for (const td of cellsOf(dataRows(t)[0]!)) {
      const i = cellsOf(dataRows(t)[0]!).indexOf(td)
      const key = list.columns[i]!
      const width = cascade(td)['width']
      if (shrink.includes(key)) {
        expect(width, `${list.label}: ${key} shrinks to its content`).toBe('1%')
      } else {
        expect(
          width,
          `${list.label}: ${key} takes the spare width, so it declares none`,
        ).toBeUndefined()
      }
    }
    expect(headers.length, `${list.label}: the columns counted`).toBe(
      list.columns.length,
    )
    cleanup()
  }
})

// ── 22 D4.27: at 320 px ─────────────────────────────────────────────────

test('22 D4.27: nothing stacks: the Actions cell holds its button alone with no text of its own, the switch and its word are one flex line, and the table scrolls inside its box with the name free to wrap', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const last = cellsOf(dataRows(t)[0]!)[headersOf(t).length - 1]!
    const kids = [...last.children]
    expect(kids, `${list.label}: the Actions cell holds one box`).toHaveLength(
      1,
    )
    expect(
      kids[0]!.classList.contains('hint-box'),
      `${list.label}: the box of the delete and its reason`,
    ).toBe(true)
    expect(
      shownText(last),
      `${list.label}: the cell shows no text, so nothing wraps under the button`,
    ).toBe('')
    cleanup()
  }
  // The switch and its word stay on one line: the wrapper is a flex line with
  // the token gap (22 D4.25's shape, read here for 320 px). The value was
  // `inline-flex` until the L1 review D1 took it off the text line, so this
  // reads the same box the sheet pin above pins.
  const main = await MODELS.open()
  const t = tableOf(main, MODELS)
  const cell = cellsOf(dataRows(t)[0]!)[columnOf(t, MODELS.valueKey)]!
  const wrap = cell.querySelector('input')!.closest('span')!
  expect(wrap, 'the switch and its word are wrapped').not.toBeNull()
  const look = cascade(wrap)
  expect(look['display'], 'one flex line').toBe('flex')
  expect(look['align-items'], 'centred on the line').toBe('center')
  expect(look['gap'], 'the token gap between them').toBe('var(--space-2)')
  cleanup()
  // The box of 10 R1.5 scrolls what is left over, and the base break wraps a
  // long name inside the column: both are here today, guards for the change.
  expect(
    values(declsOf('.table-scroll'), 'overflow-x'),
    'a table wider than its box scrolls inside it',
  ).toContain('auto')
  // `declsOf` names one selector part exactly, so the pin asks for the part the
  // name sits in: the `td` of the `th, td` rule. The `th` is not pinned here,
  // `.model-profiles th` takes the headers off wrapping on purpose.
  expect(
    values(declsOf('td'), 'overflow-wrap'),
    'the name keeps wrapping anywhere',
  ).toContain('anywhere')
})

// ── 22 D4.28: what is pinned, the sheet's absence ───────────────────────

test('22 D4.28: no rule makes a table cell, or the status cell, a flex box (the sheet, and the cascade of the model Status cell)', async () => {
  const flexed = cellRules()
    .filter((r) =>
      r.body
        .split(';')
        .some((d) => /^display:\s*flex$/.test(norm(d.replace(/^\s+/, '')))),
    )
    .map((r) => r.selector)
  expect(
    flexed,
    'rules that make a td, or .status-cell, a flex box: a flex td is no longer a table cell, so vertical-align stops applying (22 D4.25)',
  ).toEqual([])
  const main = await MODELS.open()
  const t = tableOf(main, MODELS)
  const cell = cellsOf(dataRows(t)[0]!)[columnOf(t, MODELS.valueKey)]!
  expect(
    cascade(cell)['display'],
    'the model Status cell is a table cell',
  ).not.toBe('flex')
})

// ── 22 D4.34: Used by on the server list ────────────────────────────────

test("22 D4.34: the Used by cell is one link of the instance's name, with the id as muted plain text right after it, outside the link (16 17f.4 still tells two instances named alike apart)", async () => {
  const main = await SERVERS.open({ instances: LABELLED })
  const t = tableOf(main, SERVERS)
  const cell = cellsOf(rowOf(main, SERVERS, 'default'))[
    columnOf(t, SERVERS.valueKey)
  ]!
  const links = within(cell).queryAllByRole('link')
  expect(links, 'one link, not two stacked').toHaveLength(1)
  const link = links[0]!
  expect(
    text(link),
    'the link reads the name alone: the id is not part of the name a reader hears (2.4.4)',
  ).toBe('Coding assistant')
  expect(link.getAttribute('href'), 'to the instance page').toBe(
    '#/instances/default',
  )
  const id = link.nextElementSibling
  expect(
    id,
    'the id follows the link, as its sibling: it is not inside the link',
  ).not.toBeNull()
  expect(
    id!.classList.contains('instance-id'),
    'the id keeps the span.instance-id of 16 17f.4',
  ).toBe(true)
  expect(id!.closest('a'), 'plain text: no link inside the link').toBeNull()
  expect(text(id!), 'the id text and key are unchanged').toBe(
    en('instance-id-shown', 'default'),
  )
  expect(
    shownText(cell),
    'the cell reads the name and the id, on one line',
  ).toBe(`Coding assistant ${en('instance-id-shown', 'default')}`)
  // Muted plain text on the same line: the sheet keeps the quiet colour, and
  // drops the block that stacked the id under the name in a cell (17f review
  // D1, which 22 D4.34 replaces with the id outside the link).
  expect(values(declsOf('.instance-id'), 'color'), 'the id is muted').toContain(
    'var(--muted)',
  )
  const stacked = TOP.filter((r) =>
    r.selector
      .split(',')
      .some(
        (p) => p.includes('.server-profiles') && p.includes('.instance-id'),
      ),
  ).filter((r) =>
    values(r.body.split(';').map(norm), 'display').includes('block'),
  )
  expect(
    stacked.map((r) => r.selector),
    'no rule stacks the id of a Used by cell',
  ).toEqual([])
})

test("22 D4.34: the server profile page's Used by row keeps the same shape: the name as the link, the id as plain text after it", async () => {
  startAt('#/profiles/server/qwen-27b')
  fakeBff({
    serverProfile: () =>
      reply(200, serverProfileDoc({}, { instance: 'default' })),
    instances: LABELLED,
  })
  render(<App />)
  const main = screen.getByRole('main')
  const dd = (
    await within(main).findByRole('link', {
      name: 'Coding assistant',
    })
  ).closest('dd')
  expect(dd, "the Value list's Used by row").not.toBeNull()
  const links = within(dd as HTMLElement).queryAllByRole('link')
  expect(links, 'one link').toHaveLength(1)
  expect(text(links[0]!), 'the link is the name alone').toBe('Coding assistant')
  const id = links[0]!.nextElementSibling
  expect(id, 'the id follows it, outside the link').not.toBeNull()
  expect(id!.closest('a'), 'plain text').toBeNull()
  expect(text(id!), 'the id, unchanged').toBe(
    en('instance-id-shown', 'default'),
  )
})
