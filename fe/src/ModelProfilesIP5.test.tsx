// Brief 22, slice IP5, the list half (fe/briefs/22-model-sources.md, "IP5",
// IP5.1, IP5.2, IP5.3, and the list's part of IP5.8): the plain Models list.
// The page is `src/ModelProfiles.tsx`, its rows come from
// GET /api/profiles/model.
//
// The switch and the Status column are gone (IP5.1); the list says which
// instance profiles list the model (IP5.2); a row an instance profile lists
// cannot be deleted and says so (IP5.3). The words are pinned in
// `src/test/bff.ts`'s EN map, which App.test.tsx "1a strings" compares against
// `strings/en_US.json`: what is pinned here is that the page uses them, in the
// order the brief gives, and shows nothing else of the switch.
//
// IP5.8's look below 480 px (the row keeps the name, the Used by cell and the
// −, no sideways scroll, the Used by cell wrapping) is layout: jsdom measures
// nothing, so the sheet's text is pinned at the end of this file, and the
// measurement belongs to the UX check's `model-profiles-320` state
// (fe/ux/check.js), which no test here runs.
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
import { en, fakeBff, startAt, type ModelProfile } from './test/bff.ts'

beforeEach(() => startAt('#/profiles/model'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** A profile row: `usedBy` are the instance profiles that list it (IP5.0). */
function profile(id: string, usedBy: string[] = []): ModelProfile {
  return {
    id,
    engine: 'llama.cpp',
    engineAvailable: true,
    usedBy,
    model: `/m/${id}.gguf`,
  }
}

/** The list, read from the stub. */
async function list(options: Parameters<typeof fakeBff>[0] = {}) {
  const bff = fakeBff(options)
  render(<App />)
  await screen.findByRole('heading', { name: en('model-profiles-title') })
  const main = screen.getByRole('main')
  // The read lands: the title is drawn from the route before the rows, so a
  // row is the marker. A list with no row is a list that cannot read the rows
  // of IP5.0, and that is IP5.0's red.
  await waitFor(() =>
    expect(
      main.querySelector('tbody tr'),
      'IP5.0: the list reads the rows of IP5.0, which carry usedBy and no enabled. It shows no row, so it is still reading the pre-IP5 rows, where enabled is required',
    ).not.toBeNull(),
  )
  return bff
}

/** The row of the named profile. */
function rowOf(id: string): HTMLElement {
  const main = screen.getByRole('main')
  const row = [...main.querySelectorAll<HTMLElement>('tbody tr')].find((tr) =>
    (tr.textContent ?? '').includes(id),
  )
  expect(row, `the row of ${id}`).not.toBeUndefined()
  return row!
}

/** The Used by cell: the third column of the row (IP5.1's order). */
function usedByCell(row: HTMLElement): HTMLElement {
  const cell = row.querySelector<HTMLElement>('td:nth-of-type(3)')
  expect(cell, 'the row has a third cell').not.toBeNull()
  return cell!
}

/** The row's −, named by the list's own words. */
function minus(row: HTMLElement, id: string): HTMLElement {
  return within(row).getByRole('button', {
    name: en('model-profiles-delete-row', id),
  })
}

// ── IP5.0: the read the list is built from ──────────────────────────────

test('IP5.0: the list shows the rows of a read that carries usedBy and no enabled', async () => {
  await list({
    modelProfiles: [profile('Qwen3-27B', ['default']), profile('gemma', [])],
  })
  const rows = screen
    .getByRole('table')
    .querySelectorAll('tbody tr:not(.confirm-row)')
  expect(
    [...rows].map((r) => (r.textContent ?? '').trim()),
    'IP5.0: GET /api/profiles/model answers rows with usedBy and without enabled, and the page reads them',
  ).toHaveLength(2)
})

// ── IP5.1: the plain list ───────────────────────────────────────────────

test('IP5.1: the list names its columns Model profile, Engine, Used by and Actions, in that order, and no Status', async () => {
  await list()
  const headers = [
    ...screen.getByRole('table').querySelectorAll('thead th'),
  ].map((th) => (th.textContent ?? '').trim())
  expect(
    headers,
    'IP5.1: four columns, the Actions column carrying its word for a reader only. The Status column leaves with the switch',
  ).toEqual([
    en('model-profiles-column-name'),
    en('model-profiles-column-engine'),
    en('model-profiles-column-used-by'),
    en('profiles-column-actions'),
  ])
})

test('IP5.1: the list carries no switch, and says nothing of a profile being on or off', async () => {
  await list()
  const main = screen.getByRole('main')
  expect(
    [...main.querySelectorAll('[role="switch"]')],
    'IP5.1: a model profile has no on/off state, so the list shows no switch on any row',
  ).toEqual([])
  // 22 D4.25 scoped to this page (the 22 IP5.1 ruling): the sheet keeps the
  // `.status-cell` and `.model-profiles-switch` rules for the instance page's
  // Served here cell until brief 26 removes it, so the list's own absence is
  // pinned here: the page renders no element of either class.
  for (const cls of ['status-cell', 'model-profiles-switch'])
    expect(
      main.querySelectorAll(`.${cls}`).length,
      `22 D4.25 (22 IP5.1): the list renders no element of the class ${cls}: the Status cell and the row switch are gone from this page`,
    ).toBe(0)
  // The words of the Status column and of the switch note, pinned as the
  // English they are today: IP5.8 deletes `model-profiles-column-status` and
  // `model-profiles-off-note`, so a test cannot name them by key. Their On and
  // Off words are left out: two letters sit inside other words.
  const text = main.textContent ?? ''
  for (const gone of [
    'Status',
    'A model profile switched off is kept, but its engine does not serve it and OpenCode does not offer it.',
  ])
    expect(
      text,
      `IP5.1: the words the switch and the Status column said leave the list: ${gone}`,
    ).not.toContain(gone)
})

// ── IP5.2: the Used by column ───────────────────────────────────────────

test('IP5.2: the Used by cell names the instance profiles that list the model, in the order the read gave them, each reaching its entry page', async () => {
  const bff = await list({
    modelProfiles: [profile('Qwen3-27B', ['two', 'default'])],
  })
  const cell = usedByCell(rowOf('Qwen3-27B'))
  const links = [...cell.querySelectorAll('a')].map((a) => ({
    text: (a.textContent ?? '').trim(),
    href: a.getAttribute('href'),
  }))
  expect(
    links,
    'IP5.2: each instance profile that lists the model is named, in the order the read listed them, and reaches that profile’s entry page',
  ).toEqual([
    { text: 'two', href: '#/profiles/instance/two' },
    { text: 'default', href: '#/profiles/instance/default' },
  ])
  expect(
    bff.calls.filter((c) => c.path.startsWith('/api/profiles/server')).length,
    'IP5.2: the cell is built from the list read, which carries usedBy: the page asks the bff for nothing else',
  ).toBe(0)
})

test('IP5.2: the Used by cell of a model no instance profile lists says so, in words, with no link', async () => {
  await list({ modelProfiles: [profile('gemma-3-12b-it', [])] })
  const cell = usedByCell(rowOf('gemma-3-12b-it'))
  expect(
    cell.textContent,
    'IP5.2: an unused profile names no instance profile',
  ).toContain(en('model-profiles-unused'))
  expect(
    [...cell.querySelectorAll('a')].map((a) => (a.textContent ?? '').trim()),
    'IP5.2: and the words are not a link to anywhere',
  ).toEqual([])
})

// ── IP5.3: the − of a used row ──────────────────────────────────────────

test('IP5.3: the − of a row an instance profile lists cannot act, and its description says an instance profile uses it', async () => {
  await list({ modelProfiles: [profile('Qwen3-27B', ['default'])] })
  const row = rowOf('Qwen3-27B')
  const del = minus(row, 'Qwen3-27B')
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.3: a profile an instance profile lists is not deletable, so its − is switched off',
  ).toBe('true')
  const described = del.getAttribute('aria-describedby')
  expect(
    described,
    'IP5.3: the − describes itself with its reason, as a list row’s − does (22 D4.2)',
  ).not.toBeNull()
  const reason = document.getElementById(described!)
  expect(
    reason?.textContent,
    'IP5.3: the reason names the user of the model, not a switch',
  ).toBe(en('model-profiles-delete-in-use'))
  fireEvent.click(del)
  expect(
    document.querySelector('.confirm-row'),
    'IP5.3: a click on a − that cannot act opens no confirm row',
  ).toBeNull()
})

test('IP5.3: the − of a row no instance profile lists acts, and its confirm deletes the profile', async () => {
  const bff = await list({ modelProfiles: [profile('gemma-3-12b-it', [])] })
  const row = rowOf('gemma-3-12b-it')
  const del = minus(row, 'gemma-3-12b-it')
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.3: a profile no instance profile lists is deletable',
  ).not.toBe('true')
  fireEvent.click(del)
  fireEvent.click(
    await screen.findByRole('button', { name: en('confirm-delete') }),
  )
  await waitFor(() =>
    expect(
      bff.calls.filter((c) => c.method === 'DELETE').map((c) => c.path),
      'IP5.3: the confirm sends the delete of that profile',
    ).toEqual(['/api/profiles/model/gemma-3-12b-it']),
  )
  await screen.findByText(en('model-profiles-deleted', 'gemma-3-12b-it'))
})

// ── IP5.8, the part jsdom can pin: the sheet ────────────────────────────
//
// jsdom applies no layout. These read `src/index.css` as text, the way
// Reflow.test.ts does: no rule of the Model profiles table keeps a cell on one
// line (so the Used by cell wraps inside the table's box), and the narrow
// block keeps folding the Engine column under the name, which is what leaves
// the name, the Used by cell and the − in a row below 480 px. The scrollWidth
// of the row at 320 px is the UX check's measurement, unproven here.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1]!.trim().replace(/\s+/g, ' '),
  body: m[2]!,
  depth:
    (CSS.slice(0, m.index ?? 0).match(/\{/g) ?? []).length -
    (CSS.slice(0, m.index ?? 0).match(/\}/g) ?? []).length,
}))

/** The rules that name the Model profiles table, at any nesting. */
function tableRules(): typeof RULES {
  return RULES.filter((r) => r.sel.includes('.model-profiles'))
}

test('IP5.8: no rule of the Model profiles table keeps a cell on one line, so the Used by cell wraps', () => {
  // The Actions column keeps its one-line shape (22 D4.24, pinned in
  // ListColumns.test.tsx) and the filter's button is not a cell: a *cell* kept
  // on one line is what would push the table past its box.
  const onecell = tableRules()
    .filter((r) => /white-space:\s*(nowrap|pre)/.test(r.body))
    .map((r) => r.sel.replace(/td:last-child/g, ''))
    .filter((sel) => /\btd\b/.test(sel))
  expect(
    onecell,
    'IP5.8: no Model profiles cell is kept on one line, so a Used by cell of several long instance profile names wraps inside the table’s box, as the server list’s does (1.4.10)',
  ).toEqual([])
  expect(
    RULES.filter((r) => r.sel === 'th, td')
      .map((r) => r.body.replace(/\s+/g, ' '))
      .join(' '),
    'IP5.8: every cell breaks a long word, which is what lets a Used by link wrap inside the box',
  ).toContain('overflow-wrap: anywhere')
})

test('IP5.8: below 480 px the Engine column folds under the name, leaving the name, the Used by cell and the − in the row', () => {
  const narrow = RULES.filter(
    (r) =>
      r.depth > 0 &&
      r.sel === '.model-profiles td:nth-child(2)' &&
      /display:\s*none/.test(r.body),
  )
  expect(
    narrow.length,
    'IP5.8: the Engine column is hidden in the narrow block, its word folded under the name as 22 D4.35 leaves it, so the narrow row is the name, the Used by cell and the −',
  ).toBeGreaterThan(0)
})
