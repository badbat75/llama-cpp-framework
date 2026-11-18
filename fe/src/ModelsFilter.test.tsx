// The Models page's filter, slice 2b (fe/briefs/02-models.md, section 2b):
// a search field over the model profile list at #/models, matching the id or the
// model path as the GUI does, with a status line, a Clear filter button and
// Escape. The whole app renders against the fake bff (mocked fetch).
// "2b.27" is the brief's criterion 27. The layout of 25, the look (hint and
// status in --muted, the hidden browser clear glyph of 24, the focus ring of
// 36) are checked in the designer's review, not in jsdom.
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
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  navigate,
  reply,
  startAt,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

beforeEach(() => startAt('#/models'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

function modelProfile(id: string, model: string, enabled = true): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

/** The brief's own matching example (criterion 27). */
const EXAMPLE: ModelProfile[] = [
  modelProfile('Qwen3-27B', '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  modelProfile('Old-Coder', ''),
]

/** The app at #/models; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('model-profiles-title') })
}

/** The filter field, once the table is there. */
async function field(main: HTMLElement): Promise<HTMLInputElement> {
  await table(main)
  return within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
}

function clearButton(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('button', {
    name: en('model-profiles-filter-clear'),
  })
}

/** The ids of the shown rows; [] when the table is not rendered. */
function shownIds(main: HTMLElement): string[] {
  const t = within(main).queryByRole('table', {
    name: en('model-profiles-title'),
  })
  if (t === null) return []
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
    .map((row) => (row.querySelector('td, th')?.textContent ?? '').trim())
}

/** The data rows' cells' text. */
function rows(main: HTMLElement): string[][] {
  const t = within(main).getByRole('table', {
    name: en('model-profiles-title'),
  })
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
    .map((row) =>
      [...row.querySelectorAll('td, th')].map((c) =>
        (c.textContent ?? '').trim(),
      ),
    )
}

function type(input: HTMLInputElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The filter's status line: the first status (2c.50 adds the switch one after it). */
function status(main: HTMLElement): HTMLElement {
  return within(main).getAllByRole('status')[0]!
}

/** The text of the elements an element's aria-describedby names. */
function description(el: HTMLElement): string {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ')
    .trim()
}

/** Every piece of the filter is out of the DOM. */
function noFilter(main: HTMLElement, what: string) {
  expect(within(main).queryByRole('searchbox'), what).toBeNull()
  expect(main.querySelector('input'), what).toBeNull()
  expect(
    within(main).queryByText(en('model-profiles-filter-label')),
    what,
  ).toBeNull()
  expect(
    within(main).queryByText(en('model-profiles-filter-hint')),
    what,
  ).toBeNull()
  expect(clearButton(main), what).toBeNull()
}

function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

function nameOf(el: HTMLElement): string {
  const by = el.getAttribute('aria-labelledby')
  if (by)
    return (document.getElementById(by)?.textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim()
  const text =
    el instanceof HTMLInputElement
      ? [...(el.labels ?? [])].map((l) => l.textContent).join(' ')
      : el.textContent
  return (text ?? '').replace(/\s+/g, ' ').trim()
}

// ── Failure paths first: when the filter is not shown (33) ──────────────

test('2b.33: on any error the filter is not in the DOM: no label, field, hint, button or status', async () => {
  const answers: [string, BffOptions['modelProfiles'], string][] = [
    [
      '403',
      () => reply(403, { key: 'client-not-authorized', args: [] }),
      en('api-client-not-authorized'),
    ],
    [
      '503',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '500',
      () => reply(500, { key: 'x', args: [] }),
      en('model-profiles-load-failed'),
    ],
    [
      'network',
      () => Promise.reject(new TypeError('Failed to fetch')),
      en('model-profiles-load-failed'),
    ],
    [
      'a row without model',
      () =>
        reply(200, [
          {
            id: 'a',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: true,
          },
        ]),
      en('model-profiles-load-failed'),
    ],
  ]
  for (const [what, modelProfiles, shown] of answers) {
    const { main } = open({ modelProfiles })
    await within(main).findByText(shown)
    noFilter(main, what)
    expect(within(main).queryByRole('status'), what).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2b.33: an empty list shows no filter', async () => {
  const { main } = open({ modelProfiles: [] })
  await within(main).findByText(en('model-profiles-empty'))
  noFilter(main, 'empty')
  expect(within(main).queryByRole('status')).toBeNull()
})

test('2b.33: while loading the filter is not shown; it appears with the table', async () => {
  const pending = deferred()
  const { main } = open({ modelProfiles: () => pending.promise })
  const loading = await within(main).findByText(en('model-profiles-loading'))
  noFilter(main, 'loading')
  // The loading paragraph is the only status.
  expect(within(main).getAllByRole('status')).toEqual([loading])
  await act(async () => {
    pending.resolve(reply(200, EXAMPLE))
  })
  const input = await field(main)
  expect(input.value).toBe('')
  expect(status(main).textContent).toBe('')
})

// ── Data (22) ───────────────────────────────────────────────────────────

test('2b.22: typing reads nothing: the model profiles are read once and nothing else is asked', async () => {
  const { bff, main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  const start = bff.calls.length
  for (const value of ['q', 'qw', 'qwen', 'xyz', '']) type(input, value)
  fireEvent.keyDown(input, { key: 'Enter' })
  type(input, 'coder')
  fireEvent.keyDown(input, { key: 'Escape' })
  // Let any read the page might start land.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  expect(bff.calls.slice(start)).toEqual([])
  expect(bff.to('GET', '/api/profiles/model')).toHaveLength(1)
})

// ── The field (23, 24, 26) ──────────────────────────────────────────────

test('2b.23: a visible <label for> above the field, the hint below tied by aria-describedby, no placeholder, all after the intro and before the status and the table', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  const label = within(main).getByText(en('model-profiles-filter-label'))
  expect(label.tagName).toBe('LABEL')
  expect(input.id).not.toBe('')
  expect(label.getAttribute('for')).toBe(input.id)
  expect(isHidden(label)).toBe(false)
  const hint = within(main).getByText(en('model-profiles-filter-hint'))
  expect(isHidden(hint)).toBe(false)
  expect(description(input)).toBe(en('model-profiles-filter-hint'))
  expect(input.hasAttribute('placeholder')).toBe(false)
  const intro = within(main).getByText(en('model-profiles-intro'))
  const t = await table(main)
  expect(before(intro, label), 'label after the intro').toBe(true)
  expect(before(label, input), 'label above the field').toBe(true)
  expect(before(input, hint), 'hint below the field').toBe(true)
  expect(before(hint, status(main)), 'status after the field row').toBe(true)
  expect(before(status(main), t), 'status before the table').toBe(true)
})

test('2b.24: an <input type="search"> with autocomplete off and spellcheck false, outside any form and with no search landmark', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  expect(input.tagName).toBe('INPUT')
  expect(input.getAttribute('type')).toBe('search')
  expect(input.getAttribute('autocomplete')).toBe('off')
  expect(input.getAttribute('spellcheck')).toBe('false')
  expect(input.closest('form')).toBeNull()
  expect(screen.queryByRole('search')).toBeNull()
})

test('2b.26: the field, its hint and the status read as the brief gives them, and filtering writes nothing', async () => {
  const { bff, main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  type(input, 'qwen')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  expect([
    nameOf(input),
    description(input),
    status(main).textContent ?? '',
    clearButton(main)?.textContent?.trim() ?? '',
  ]).toEqual([
    en('model-profiles-filter-label'),
    en('model-profiles-filter-hint'),
    en('model-profiles-filter-count', '1', '2'),
    en('model-profiles-filter-clear'),
  ])
  expect(bff.calls.filter((c) => c.method !== 'GET')).toEqual([])
})

// ── What matches (27, 28) ───────────────────────────────────────────────

test("2b.27: the GUI's rule on the brief's examples: id or model, lowercased, holds the whole text as typed", async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  expect(shownIds(main), 'empty field').toEqual(['Qwen3-27B', 'Old-Coder'])
  const cases: [string, string[]][] = [
    ['qwen', ['Qwen3-27B']],
    ['Q8_0', ['Qwen3-27B']],
    ['coder', ['Old-Coder']],
    ['-', ['Qwen3-27B', 'Old-Coder']],
    ['27b q8', []],
    [' qwen', []],
    // Matched by the model path alone, separators as typed.
    ['gguf', ['Qwen3-27B']],
    ['/m/qwen/', ['Qwen3-27B']],
    // Upper case typed matches lower case held.
    ['OLD-C', ['Old-Coder']],
    ['', ['Qwen3-27B', 'Old-Coder']],
  ]
  for (const [value, ids] of cases) {
    type(input, value)
    expect(shownIds(main), JSON.stringify(value)).toEqual(ids)
  }
})

test('2b.27: no trimming of a trailing space, no wildcard or pattern', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  for (const value of ['qwen ', 'q*n', 'qwen.*', '^qwen', 'q?wen', '[q]']) {
    type(input, value)
    expect(shownIds(main), JSON.stringify(value)).toEqual([])
  }
})

test('2b.28: shown rows keep the reply order and their cells; switched-off rows are filtered like the others; nothing is highlighted', async () => {
  const { main } = open({
    modelProfiles: [
      modelProfile('zeta-a', ''),
      modelProfile('Alpha', '', false),
      modelProfile('mid', '/x/none.gguf'),
      // Matched by its model path only.
      modelProfile('beto', '/x/A.gguf', false),
    ],
  })
  const input = await field(main)
  type(input, 'a')
  expect(rows(main)).toEqual([
    ['zeta-a', 'llama.cpp', en('model-profiles-status-on')],
    ['Alpha', 'llama.cpp', en('model-profiles-status-off')],
    ['beto', 'llama.cpp', en('model-profiles-status-off')],
  ])
  const t = await table(main)
  expect(t.querySelectorAll('mark, b, strong, em')).toHaveLength(0)
  type(input, 'alpha')
  expect(rows(main)).toEqual([
    ['Alpha', 'llama.cpp', en('model-profiles-status-off')],
  ])
})

// ── When it updates, what is heard (29, 30) ─────────────────────────────

test('2b.29: each change updates the table at once; Enter does nothing; focus and route stay', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  input.focus()
  type(input, 'q')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  type(input, 'qx')
  expect(shownIds(main)).toEqual([])
  // Deleting a character, as the browser's own Escape clear would.
  type(input, 'q')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  fireEvent.keyDown(input, { key: 'Enter' })
  fireEvent.keyUp(input, { key: 'Enter' })
  expect(input.value).toBe('q')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  expect(document.activeElement).toBe(input)
  expect(window.location.hash).toBe('#/models')
})

test('2b.30, 2c.50: the filter status paragraph, in the page before its text changes: empty, the count, or no-match with no table and the note kept', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  const line = status(main)
  expect(line.tagName).toBe('P')
  expect(line.getAttribute('role')).toBe('status')
  expect(line.textContent).toBe('')

  type(input, 'qwen')
  expect(status(main)).toBe(line)
  expect(line.textContent).toBe(en('model-profiles-filter-count', '1', '2'))

  type(input, '-')
  expect(status(main)).toBe(line)
  expect(line.textContent).toBe(en('model-profiles-filter-count', '2', '2'))

  type(input, 'nothing-like-it')
  expect(status(main)).toBe(line)
  expect(line.textContent).toBe(en('model-profiles-no-match'))
  expect(within(main).queryByRole('table')).toBeNull()
  expect(within(main).queryAllByRole('columnheader')).toHaveLength(0)
  const note = within(main).getByText(en('model-profiles-off-note'))
  expect(before(line, note)).toBe(true)
  // The field stays, so the user can change the text.
  expect(within(main).getByRole('searchbox')).toBe(input)

  type(input, '')
  expect(status(main)).toBe(line)
  expect(line.textContent).toBe('')
  expect(shownIds(main)).toEqual(['Qwen3-27B', 'Old-Coder'])
})

test('2b.30: the count is of the shown rows and the total of the reply', async () => {
  const { main } = open({
    modelProfiles: [
      modelProfile('a-1', ''),
      modelProfile('a-2', '', false),
      modelProfile('b-1', '/m/a.gguf'),
      modelProfile('c-1', ''),
      modelProfile('d-1', ''),
    ],
  })
  const input = await field(main)
  type(input, 'a')
  expect(status(main).textContent).toBe(
    en('model-profiles-filter-count', '3', '5'),
  )
})

// ── Clearing (31, 32) ───────────────────────────────────────────────────

test('2b.31: Clear filter is a type="button" after the field, only while it holds text; pressing it empties the field, shows every row, empties the status and focuses the field', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  expect(clearButton(main), 'not shown with an empty field').toBeNull()
  type(input, 'coder')
  const button = clearButton(main)!
  expect(button).not.toBeNull()
  expect(button.tagName).toBe('BUTTON')
  expect(button.getAttribute('type')).toBe('button')
  expect(before(input, button)).toBe(true)
  expect(before(button, status(main))).toBe(true)
  button.focus()
  fireEvent.click(button)
  expect(input.value).toBe('')
  expect(shownIds(main)).toEqual(['Qwen3-27B', 'Old-Coder'])
  expect(status(main).textContent).toBe('')
  expect(clearButton(main)).toBeNull()
  await waitFor(() => expect(document.activeElement).toBe(input))
})

test('2b.32: Escape in the field with text clears it as the button does, focus staying; with the field empty it does nothing', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  input.focus()
  type(input, 'Q8_0')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  fireEvent.keyDown(input, { key: 'Escape' })
  expect(input.value).toBe('')
  expect(shownIds(main)).toEqual(['Qwen3-27B', 'Old-Coder'])
  expect(status(main).textContent).toBe('')
  expect(clearButton(main)).toBeNull()
  expect(document.activeElement).toBe(input)

  fireEvent.keyDown(input, { key: 'Escape' })
  expect(input.value).toBe('')
  expect(shownIds(main)).toEqual(['Qwen3-27B', 'Old-Coder'])
  expect(status(main).textContent).toBe('')
  expect(document.activeElement).toBe(input)
  expect(window.location.hash).toBe('#/models')
})

// ── Not kept (34) ───────────────────────────────────────────────────────

test('2b.34, 2d.62: the text is in no URL and no storage, and the page reopens with the kept text', async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  type(input, 'zqwxv')
  expect(window.location.href).not.toContain('zqwxv')
  for (const store of [localStorage, sessionStorage]) {
    for (let i = 0; i < store.length; i++) {
      const key = store.key(i) ?? ''
      expect(key + (store.getItem(key) ?? '')).not.toContain('zqwxv')
    }
  }
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/models')
  const again = (await within(main).findByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  })) as HTMLInputElement
  // 2d.62: the text lives in the app's memory, so it outlives the page.
  expect(again.value).toBe('zqwxv')
  expect(shownIds(main)).toEqual([])
  expect(status(main).textContent).toBe(en('model-profiles-no-match'))
  expect(clearButton(main)).not.toBeNull()
})

// ── Keyboard and screen reader (35, 36) ─────────────────────────────────

test('2b.35, 2c.53, 2d.60: with text, Tab reaches the field, Clear filter, then the shown row link and switch; focus on arrival stays on the h1', async () => {
  // 2c.53 replaces 2b.35: the administrator's switches follow Clear filter.
  // Focus moves to the h1 only after a route change, never on a first load.
  startAt('#/')
  const { main } = open({ modelProfiles: EXAMPLE })
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/models')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('models-title'),
  })
  const input = await field(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  type(input, 'q')
  // 2d.60: each shown row's name is a link; it precedes the row's switch.
  const order = tabbable().map(nameOf)
  expect(order.slice(-5)).toEqual([
    en('nav-models'),
    en('model-profiles-filter-label'),
    en('model-profiles-filter-clear'),
    'Qwen3-27B',
    'Qwen3-27B',
  ])
  const t = await table(main)
  expect(tabbable().filter((el) => t.contains(el))).toEqual([
    ...within(t).getAllByRole('link'),
    ...within(t).getAllByRole('switch'),
  ])
})

test("2b.36: the field's accessible name is the label, its role searchbox, its description the hint", async () => {
  const { main } = open({ modelProfiles: EXAMPLE })
  const input = await field(main)
  expect(input.getAttribute('role') ?? 'searchbox').toBe('searchbox')
  expect(nameOf(input)).toBe(en('model-profiles-filter-label'))
  expect(input.getAttribute('aria-label')).toBeNull()
  expect(description(input)).toBe(en('model-profiles-filter-hint'))
})
