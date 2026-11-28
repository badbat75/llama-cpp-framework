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

function modelProfile(id: string, usedBy: string[]): ModelProfile {
  // `enabled` is the key the page reads today, and a row without it is a row
  // the page reads as malformed; slice IP5 takes the key out of the read, so
  // the rows carry it beside `usedBy`. It is set to "an instance profile lists
  // this model", which is the row today's page blocks (its profile is on) and
  // the row IP5 blocks (IP5.3): the same row under both readings.
  return {
    id,
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: usedBy.length > 0,
    usedBy,
    model: '',
  }
}

/** One row an instance profile lists, one it does not: only the second row's
 *  delete can act (22 IP5.3). */
const MODEL_LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', ['default']),
  modelProfile('Old-Coder', []),
]

function serverRow(id: string, instance: string | null): ServerProfile {
  return { id, engine: 'llama.cpp', port: 8580, tags: [], instance }
}

/** One profile an instance runs, one free: only the free row can act. */
const SERVER_ROWS: ServerProfile[] = [
  serverRow('default', 'default'),
  serverRow('qwen-27b', null),
]

/** The instance of 22 D4.34 as 22 D4.37 amends it: a label, so its name is not
 *  its id, and a name no other instance of the stub's list carries, so its
 *  Used by link carries no id. */
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
  valueKey: 'model-profiles-column-used-by',
  columns: [
    'model-profiles-column-name',
    'model-profiles-column-engine',
    'model-profiles-column-used-by',
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

test('22 D4.24 (as 22 IP5.2 amends the model list): the Actions column is as narrow as its button and its cell ends the row, the model list\u2019s Used by column keeps the spare width, and the hint box is as wide as the button', async () => {
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
  // The model list's Used by column keeps the spare width and keeps wrapping
  // (22 IP5.2): the cell lists the instance profiles that use the model, so it
  // is never squeezed to its content the way the Actions column is.
  const main = await MODELS.open()
  const t = tableOf(main, MODELS)
  const used = cellsOf(dataRows(t)[0]!)[columnOf(t, MODELS.valueKey)]!
  expect(
    cascade(used)['width'],
    'the Used by column declares no width, so the spare width is its own',
  ).toBeUndefined()
  expect(
    cascade(used)['white-space'],
    'the Used by cell is left to wrap',
  ).toBeUndefined()
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

// ── 22 D4.25, as 22 IP5.1 retires it, and item 8's wrap ─────────────────

test('22 D4.25 (as 22 IP5.1 retires it): no rule lays out a status cell or a row switch inside the Models list, the sheet lays out no off row, and the Models list\u2019s Used by cell keeps the break every table cell inherits (IP5.1, IP5.2, 1.4.10)', () => {
  // The L1 review's rule centred the row's switch and its word in the Status
  // cell. IP5.1 takes the Status column, the switch, its status region, its
  // alert, its note and the off row away from the Models list. The
  // `.status-cell` and `.model-profiles-switch` rules stay in the sheet for
  // the instance page's Served here cell (17e.4a), which keeps both until
  // brief 26 (IP6) removes that section, so the pin is scoped to the list:
  // no rule styles these classes inside `.model-profiles`, and
  // ModelProfilesIP5 pins that the list renders no element of either.
  const named = (needle: string): string[] =>
    TOP.filter((r) =>
      r.selector.split(',').some((p) => norm(p).includes(needle)),
    ).map((r) => norm(r.selector))
  const inList = (sels: string[]): string[] =>
    sels.filter((s) => /\.model-profiles(?=$|[\s>])/.test(s))
  expect(
    inList(named('.status-cell')),
    'rules that style a status cell inside the Models list, the column IP5.1 retires: the class stays for the instance page, scoped never to the list',
  ).toEqual([])
  expect(
    inList(named('.model-profiles-switch')),
    'rules that style a row switch inside the Models list, IP5.1 retires both: the class stays for the instance page, scoped never to the list',
  ).toEqual([])
  expect(
    named('.model-profiles-note'),
    'rules that style the note under the table: the note is `model-profiles-off-note`, which IP5.1 takes away',
  ).toEqual([])
  expect(
    named('tr.off'),
    'rules that style a switched-off row: `models.ini.disabled` goes with the switch (IP5.1)',
  ).toEqual([])
  // Item 8: the Used by cell wraps at the cell's width. The break is the one
  // every table cell inherits (the `td` of the `th, td` rule, 22 D4.27), and
  // the pin is that nothing takes the new column off it: the Engine column
  // keeps its words unbroken (`td:nth-child(2)`), the Used by column does not.
  expect(
    values(declsOf('td'), 'overflow-wrap'),
    'every table cell breaks anywhere, so the Used by cell wraps at the cell\u2019s width',
  ).toContain('anywhere')
  expect(
    TOP.filter((r) =>
      r.selector.includes('.model-profiles td:nth-child(3)'),
    ).flatMap((r) =>
      values(
        r.body.split(';').map((d) => norm(d)),
        'overflow-wrap',
      ),
    ),
    'no rule takes the Models list\u2019s third column, Used by, off the inherited break',
  ).toEqual([])
})

// ── 22 D4.26: no dead space ─────────────────────────────────────────────

test('22 D4.26 (as 22 IP5.2 amends the model list): the table fills the card and the spare width goes to the text columns: no width on the name, the engine, the tags, the status successor or the Used by cell, a 1% width on the actions cell', async () => {
  expect(
    values(declsOf('table'), 'width'),
    'the table fills the card',
  ).toContain('100%')
  for (const list of LISTS) {
    const main = await list.open()
    const t = tableOf(main, list)
    const headers = headersOf(t)
    const shrink = ['profiles-column-actions']
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

test('22 D4.27 (as 22 IP5.1 amends the model list): nothing stacks: the Actions cell holds its button alone with no text of its own, and the table scrolls inside its box with the name free to wrap', async () => {
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

test('22 D4.28 (as 22 IP5.2 amends the model list): no rule makes a table cell, or the status cell, a flex box (the sheet, and the cascade of the model list\u2019s Used by cell)', async () => {
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
    'the model list\u2019s Used by cell is a table cell',
  ).not.toBe('flex')
})

// ── 22 D4.34 as 22 D4.37 amends it: Used by on the server list ──────────

test("22 D4.34 (as 22 D4.37 amends it): the Used by cell is one link of the instance's name and nothing after it, the id only where two instances share that name", async () => {
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
  // 22 D4.37: the id follows the link only where names are shared. This stub's
  // instance list has one instance, so its name is unique and the cell is the
  // link alone. The shared name is L2InstanceLink.test.tsx.
  expect(
    cell.querySelector('.instance-id'),
    '22 D4.37: no id beside a name no other instance shares',
  ).toBeNull()
  expect(shownText(cell), '22 D4.37: the cell is the name alone').toBe(
    'Coding assistant',
  )
  // Muted plain text on the same line, when it is shown: the sheet keeps the
  // quiet colour, and drops the block that stacked the id under the name in a
  // cell (17f review D1, which 22 D4.34 replaces with the id outside the link).
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

test("22 D4.34 (as 22 D4.37 amends it): the server profile page's Used by row keeps the same shape: the name as the link, and nothing after it where the name is unique", async () => {
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
  expect(
    dd!.querySelector('.instance-id'),
    '22 D4.37: no id beside a name no other instance shares',
  ).toBeNull()
  expect(
    shownText(dd as HTMLElement),
    '22 D4.37: the row is the name alone',
  ).toBe('Coding assistant')
})
