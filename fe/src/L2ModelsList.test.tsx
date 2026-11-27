// Brief 22, slice L2 (fe/briefs/22-model-sources.md, "L2 acceptance list for
// the tester" items 8, 9, 10 and 12): 22 D4.33, the filter's hint, 22 D4.35,
// the Engine column folded under the name below 480 px, 22 D4.36, the word
// beside every row switch, and L2.A1, the create `+` staying after the list.
//
// The DOM half only. What jsdom cannot measure is left to the designer's
// measurement of the UX check's `model-profiles-320` state (L1 review's D4.35:
// the table needing no sideways scroll inside its box at 320 px, and the trash
// can standing inside the box's right edge), and to Reflow.test.ts, where the
// sheet's `@media (max-width: 479.98px)` block is pinned: the Engine column
// hidden there, its header visually hidden with its sort kept, and the name
// cell's second line shown. The DOM always holds both copies, so the sheet
// alone says which one the eye reads: that is why a test of this file reads
// the copy inside the name cell at any width and asks the sheet for its look.
//
// D4.33's text itself is pinned in `fe/src/test/bff.ts`'s EN map, which
// App.test.tsx "1a strings" compares against `strings/en_US.json`: the hint
// words land there, and what is pinned here is that the field describes itself
// with them and that they stand as one line.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  before,
  en,
  fakeBff,
  rowName,
  startAt,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

// ── The sheet, read as text (Reflow.test.ts's reader, a small local copy) ──
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
 * element or on one of its parents up to `stop`: D4.35 says the line is muted,
 * not that a class of its own carries the colour. */
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

// ── The list page ─────────────────────────────────────────────────────────
/** Two profiles of one engine, one of them without the engine: D4.35's
 * `model-profiles-engine-not-available` has to follow the name too. */
const PAIR: ModelProfile[] = [
  {
    id: 'gemma-3-12b-it',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: false,
    model: '/var/lib/llama.cpp/models/gemma-3-12b-it.gguf',
  },
  {
    id: 'qwen3-235b',
    engine: 'strata',
    engineAvailable: false,
    enabled: true,
    model: '',
  },
]

/** The Models list, its table read. */
async function modelsList(options: BffOptions = {}): Promise<HTMLElement> {
  startAt('#/profiles/model')
  fakeBff({ modelProfiles: PAIR, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  return within(main).findByRole('table', {
    name: en('model-profiles-title'),
  }) as Promise<HTMLElement>
}

/** The row of one profile. */
const rowOf = (table: HTMLElement, id: string): HTMLElement =>
  within(table).getByRole('row', { name: new RegExp(id) })

/** The cells of a row, and the header of one column. */
const cells = (row: HTMLElement): HTMLElement[] =>
  within(row).getAllByRole('cell')
const engineHeader = (table: HTMLElement): HTMLElement =>
  within(table).getByRole('columnheader', {
    name: new RegExp(en('model-profiles-column-engine')),
  })

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Item 8: 22 D4.33, the filter hint ─────────────────────────────────────

test('22 L2 D4.33: the filter describes itself with the one short hint, standing as one line', async () => {
  startAt('#/profiles/model')
  fakeBff({ modelProfiles: PAIR })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  const field = within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  })
  const hint = main.querySelector('#model-profiles-filter-hint')
  expect(hint, 'the hint the field names is there').not.toBeNull()
  expect(
    hint!.textContent,
    'the hint says, in one line, what the field matches',
  ).toBe(en('model-profiles-filter-hint'))
  expect(
    hint!.tagName,
    'one element, not two paragraphs of the old two sentences',
  ).toBe('P')
  expect(
    field.getAttribute('aria-describedby'),
    'the field names it',
  ).toContain('model-profiles-filter-hint')
  expect(en('model-profiles-filter-hint'), 'the words D4.33 gives').toBe(
    'Name or model file; case does not matter.',
  )
})

// ── Item 8's D2 review: the filter's status line reserves no room ─────────

/** Every rule of the sheet that reaches `el`. A selector jsdom's engine cannot
 *  parse is skipped (a `:has()` of another element, a pseudo-element). */
const reaching = (el: Element): { sel: string; body: string }[] =>
  RULES.filter((r) => {
    try {
      return el.matches(r.sel)
    } catch {
      return false
    }
  })

/** Every rule of the sheet that reaches `el` and declares `name`, as
 *  "selector: value", so a red names the rule to fix. */
const declaring = (el: Element, name: string): string[] =>
  reaching(el)
    .filter((r) => prop(r.body, name) !== '')
    .map((r) => `${r.sel}: ${name}: ${norm(prop(r.body, name))}`)

/** The values the sheet declares for `name` on the class `sel` (a rule's
 *  selector list is matched part by part, as the class of the element the
 *  brief names). The switches' line shows only after a switch, so the sheet
 *  is asked, not the page. */
/** The value a rule's body gives one vertical margin, the shorthands expanded:
 *  `margin: a` is `a` everywhere, `margin: a b` puts `b` above and below,
 *  `margin: a b c` and `margin: a b c d` put `a` above and `c` below;
 *  `margin-block: a` is `a` above and below. */
const marginValue = (body: string, side: 'top' | 'bottom'): string => {
  const long = prop(body, `margin-${side}`)
  if (long !== '') return long
  const block = prop(body, 'margin-block')
  if (block !== '') return block.split(/\s+/)[0] ?? ''
  const m = prop(body, 'margin')
  if (m === '') return ''
  const v = m.split(/\s+/)
  if (v.length === 1) return v[0]
  if (v.length === 2) return v[1]
  return side === 'top' ? v[0] : (v[2] ?? v[1])
}

/** The value the cascade gives `el` for one vertical margin: every rule
 *  reaching it that declares the margin (directly or by a shorthand), ordered
 *  as the cascade orders them (the reader of ListColumns.test.tsx: the
 *  specificity counted by `.` and `#`, a tie on source order), the last
 *  winning. The sheet's global `p { margin: 0 0 var(--space-3) }` reaches the
 *  filter's line (0,0,1), so the D2 review's rule is not that no rule declares
 *  a margin: it is that 0 wins for it, its fix being a declaration on the
 *  class (0,1,0). A pin of "no rule reaching it declares a margin" would red a
 *  correct fix. */
const marginWins = (el: Element, side: 'top' | 'bottom'): string => {
  const hits = reaching(el)
    .map((r, i) => ({
      i,
      spec: (r.sel.match(/[.#]/g) ?? []).length,
      value: marginValue(r.body, side),
    }))
    .filter((h) => h.value !== '')
    .sort((a, b) => a.spec - b.spec || a.i - b.i)
  return hits.length === 0 ? '' : norm(hits[hits.length - 1].value)
}

const classDeclares = (sel: string, name: string): string[] =>
  RULES.filter((r) =>
    r.sel
      .split(',')
      .map((part) => norm(part))
      .includes(sel),
  )
    .filter((r) => prop(r.body, name) !== '')
    .map((r) => norm(prop(r.body, name)))

test('22 D4.33 (as its D2 review amends it): the filter’s status line reserves no room when it is empty, and the switches’ status line keeps its one line', async () => {
  startAt('#/profiles/model')
  fakeBff({ modelProfiles: PAIR })
  render(<App />)
  const main = screen.getByRole('main')
  const table = await within(main).findByRole('table', {
    name: en('model-profiles-title'),
  })
  // The filter's status line, the list's read, before the table. With no
  // filter typed it says nothing, which is the case of the gap the review
  // measures (35 px under the hint, where D4.33 asks 16 px).
  const line = within(main)
    .getAllByRole('status')
    .find((el) => before(el, table))
  expect(
    line,
    'the filter’s status line is there, the list’s read (2c.7)',
  ).toBeDefined()
  expect(
    line!.textContent,
    'the list read with no filter typed says nothing',
  ).toBe('')
  // jsdom lays nothing out, so the review's measurement is pinned as the sheet:
  // the line keeps no height of its own (H1a.12's list, HearthTokens.test.ts,
  // drops it too), and no margin above and below, so an empty line adds
  // nothing: the filter block's --space-4, 16 px, is all that separates the
  // hint from the table (D4.33).
  expect(
    declaring(line!, 'min-height'),
    '22 D4.33 review D2: no rule gives the filter’s status line a floor of its own',
  ).toEqual([])
  for (const side of ['top', 'bottom'] as const)
    expect(
      marginWins(line!, side),
      `22 D4.33 review D2: the cascade leaves the filter’s status line no margin above or below (the global p rule reaches it, so the rule is that the class's 0 wins for it, ${side})`,
    ).toBe('0')
  // 17d.5a is NOT undone: the switch's line, which stands between the table
  // and the offer that replaces it, keeps the one line that stops the jump.
  expect(
    classDeclares('.model-profiles-switch-status', 'min-height'),
    '17d.5a: the switches’ status line keeps its 1lh floor',
  ).toContain('1lh')
})

// ── Item 9: 22 D4.35, the Engine under the name ───────────────────────────

test('22 L2 D4.35: each row carries its engine inside its first cell, under the name, as a muted second line', async () => {
  const table = await modelsList()
  const row = rowOf(table, 'gemma-3-12b-it')
  const [nameCell, engineCell] = cells(row)
  expect(
    within(nameCell).getByText('llama.cpp'),
    'the engine stands in the cell of the name, so the width of one column can hide it with the column',
  ).toBeDefined()
  expect(
    before(nameCell, engineCell),
    'the name cell is the row first cell, the engine copy sits under the name',
  ).toBe(true)
  expect(
    styled(
      within(nameCell).getByText('llama.cpp'),
      'color',
      'var(--muted)',
      nameCell,
    ),
    'the second line is muted (D4.35)',
  ).toBe(true)
})

test('22 L2 D4.35: an engine the node has not got is spelled out under the name too', async () => {
  const table = await modelsList()
  const row = rowOf(table, 'qwen3-235b')
  const [nameCell] = cells(row)
  expect(
    within(nameCell).getByText(
      en('model-profiles-engine-not-available', 'strata'),
    ),
    'the not-available words follow the name, as the column says them',
  ).toBeDefined()
})

test('22 L2 D4.35: the Engine column keeps its words in its own cell, and its header keeps the sort', async () => {
  const table = await modelsList()
  const [nameCell, engineCell] = cells(rowOf(table, 'gemma-3-12b-it'))
  expect(
    engineCell.textContent,
    'at 480 px and wider the Engine column reads as today (D4.35): the words are in the column',
  ).toContain('llama.cpp')
  expect(
    nameCell.closest('tr') === engineCell.closest('tr'),
    'the two copies are of one row',
  ).toBe(true)
  const head = engineHeader(table)
  expect(
    head.getAttribute('aria-sort'),
    'the column is not sorted at first',
  ).toBeNull()
  fireEvent.click(within(head).getByRole('button'))
  expect(
    head.getAttribute('aria-sort'),
    'the header button sorts by engine, so the sort stays reachable when the column is hidden (D4.35)',
  ).toBe('ascending')
  fireEvent.click(within(head).getByRole('button'))
  expect(head.getAttribute('aria-sort'), 'and turns the way round').toBe(
    'descending',
  )
})

test('22 L2 D4.35: the row keeps naming itself by the id alone: the engine line sits outside the element that names the row, and the switch, the name link and the row delete carry the id and nothing else', async () => {
  // The designer's ruling of 2026-11-27, on 22 D4.35: the second line is the
  // eye's read, not the name. The element the row's controls point at for
  // their name is the name alone, so the line lives beside it in the cell, and
  // the id alone keeps naming the switch (2c.38), the link (2d.59a) and the
  // row delete (22 D4.1).
  const table = await modelsList()
  for (const p of PAIR) {
    const row = rowOf(table, p.id)
    const [nameCell] = cells(row)
    const engine = within(nameCell).getByText(
      p.engineAvailable
        ? p.engine
        : en('model-profiles-engine-not-available', p.engine),
    )
    const sw = within(row).getByRole('switch')
    const labelId = sw.getAttribute('aria-labelledby')
    expect(
      labelId,
      'the switch is labelled by an element of its row',
    ).toBeTruthy()
    const label = document.getElementById(labelId!)
    expect(label, 'the element that names the switch is there').not.toBeNull()
    expect(
      nameCell.contains(label as Element),
      'it is in the row’s name cell',
    ).toBe(true)
    expect(
      (label as Element).contains(engine),
      'the engine line stands outside it, so it adds nothing to the name',
    ).toBe(false)
    expect(
      nameCell.contains(engine),
      'and the name cell is still what shows it (22 D4.35)',
    ).toBe(true)
    expect(
      rowName(label as Element),
      'the element that names the row says the id, and nothing else',
    ).toBe(p.id)
    expect(
      within(row).getByRole('switch', { name: p.id }),
      'the switch carries the id alone',
    ).toBe(sw)
    if (p.engineAvailable) {
      // The name link (2d.59a: an unreadable id is plain text, no link).
      expect(
        within(nameCell).getByRole('link', { name: p.id }),
        'the name link carries the id alone',
      ).toBeDefined()
    }
    expect(
      within(row).getByRole('button', {
        name: en('model-profiles-delete-row', p.id),
      }),
      'the row delete is named with the id alone (22 D4.1)',
    ).toBeDefined()
  }
})

// ── Item 10: 22 D4.36, the word beside the switch ─────────────────────────

test('22 L2 D4.36: every row switch says On or Off beside it, the cue that is not a colour', async () => {
  const table = await modelsList()
  const on = cells(rowOf(table, 'qwen3-235b'))[2]
  const off = cells(rowOf(table, 'gemma-3-12b-it'))[2]
  expect(
    within(on).getByText(en('model-profiles-status-on')),
    'the row whose switch is on says On',
  ).toBeDefined()
  expect(
    within(off).getByText(en('model-profiles-status-off')),
    'the row whose switch is off says Off',
  ).toBeDefined()
  // The row switch is a checkbox input that plays the switch role, so its
  // state is the input checked property, not an aria-checked attribute.
  expect(
    (within(on).getByRole('switch') as HTMLInputElement).checked,
    'the word matches the switch it stands beside',
  ).toBe(true)
  expect(
    (within(off).getByRole('switch') as HTMLInputElement).checked,
    'and this one too',
  ).toBe(false)
})

// ── Item 12: L2.A1, the + stays after the list ────────────────────────────

test('22 L2.A1: the create + of the Models list stands after the table, and is the round + of the lists', async () => {
  const table = await modelsList()
  const main = screen.getByRole('main')
  const plus = within(main).getByRole('button', {
    name: en('model-profiles-new'),
  })
  expect(
    before(table, plus),
    'the + follows the table (22 C.1a, kept by L2.A1: the labelled New of D4.29 is dropped)',
  ).toBe(true)
  expect(plus.className, 'the round + of a list').toContain('ot-add')
})

test('22 L2.A1: the create + of the Server profiles list stands after the table, and New instance after the tiles', async () => {
  startAt('#/profiles/server')
  fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  const table = await within(main).findByRole('table', {
    name: en('server-profiles-title'),
  })
  const plus = within(main).getByRole('button', {
    name: en('server-profiles-new'),
  })
  expect(
    before(table, plus),
    'the + follows the Server profiles table (17b2, kept by L2.A1)',
  ).toBe(true)
  expect(plus.className, 'the round + of a list').toContain('ot-add')
  cleanup()

  startAt('#/')
  fakeBff()
  render(<App />)
  const overview = screen.getByRole('main')
  const tiles = await within(overview).findByRole('list', { hidden: true })
  const add = await within(overview).findByRole('button', {
    name: en('instances-new'),
  })
  expect(
    before(tiles, add),
    'New instance + follows the list of tiles (L2.A1: the same rule as the lists)',
  ).toBe(true)
  expect(add.className, 'the round + of a list').toContain('ot-add')
})
