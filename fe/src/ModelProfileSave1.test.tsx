// The smallest end-to-end save of a model profile (fe/briefs/02h-model-profile-save.md,
// sub-slice 2h.1, criteria 130 to 134): the Embeddings card's two controls
// become editable and everything else stays disabled; the read needs
// `revision`, and the save sends it back with only the keys the user changed
// (D-a, D-b); the written-as line follows the field's state, and the
// full-value line is only for a disabled control. What is sent is read from
// the body of the PUT the fake bff recorded, asked for by a click on the save
// bar's Save changes. The bar's own DOM, Discard, the pending state, the
// 200's messages, the answers that are not a shaped 200, the leave guard and
// the tab order are 135 to 141, in ModelProfileSave2.test.tsx; the keyed answers
// (409, 422) are 2h.2. The cards, their fields and the reading of a value are
// 2e (ModelProfileCards1 to ModelProfileCards5.test.tsx, ModelProfileFlags.test.tsx).
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
  deferred,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const POOLING = en('model-profile-pooling')

/** An unlisted Pooling value past the 20 characters a select holds: the extra
 * option of 87, so the field's loaded state is one the select does not list
 * (133: picking it again makes the field unchanged, so it is never sent). */
const EXTRA_POOL = 'Last-Attention-Pool-X'

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileFlags.test.tsx) ───────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page: the h1 of the model and the Assets card. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The loaded page with `values` holding exactly `values`. */
function page(values: Record<string, string>) {
  return loaded({
    modelProfile: () => reply(200, modelProfileDoc({}, { values })),
  })
}

/** An element's text, with the page's runs of whitespace folded. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

/** The written-as line of `key` (2e's 124), and the full-value line (2e's 118). */
function writtenOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#model-profile-written-${key}`)
}

/** The ids the control's `aria-describedby` names, in order (124b). */
function describedIds(control: Element): string[] {
  return (control.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** True when the button cannot be used: disabled, or aria-disabled. */
function inactive(el: HTMLElement): boolean {
  return (
    (el as HTMLButtonElement).disabled ||
    el.getAttribute('aria-disabled') === 'true'
  )
}

/**
 * The save bar's status line. The GPUs card keeps its own status region
 * in the DOM from its first render (brief 14 GD1.9), so the bar's line is
 * picked inside the bar, not by counting the page's status regions (section
 * 6 of fe/briefs/14-gpu-distribution.md, ruling of 2026-11-21).
 */
function status(main: HTMLElement): HTMLElement {
  within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The `values` object of the n-th recorded PUT body, and its whole body. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as {
    values: Record<string, unknown>
  }
  return body.values
}

function sentBody(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  return bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
}

// ── 132a: the extra option is built from the LOADED value ───────────────

/** 132a's Pooling value: a value the select does not list, short enough that
 * its option shows all of it (no full-value line to read in its place). */
const WEIRD = 'weird-pool'

/** The Pooling options the row lists (2e's Embeddings table), with no extra
 * option added: the list a select whose loaded value is in it keeps. */
const POOL_LISTED = ['default', 'none', 'mean', 'cls', 'last', 'rank']

/** The option values of `label`'s select, in the order the control lists them. */
function optionValues(main: HTMLElement, label: string): string[] {
  return [...selectOf(main, label).options].map((o) => o.value)
}

/** The bar's alert reads `text`: its own role="alert" (01's 30a). */
function expectAlert(main: HTMLElement, text: string) {
  const el = within(main).getByText(text)
  expect(el.closest('[role="alert"]'), `${text}: in a role="alert"`).not.toBe(
    null,
  )
}

/** The loaded page with revision `r1`, `values`, and `answer` for the save. */
function form(
  values: Record<string, string>,
  answer?: BffOptions['modelProfileSave'],
) {
  return loaded({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values })),
    modelProfileSave: answer,
  })
}

// ── 130: what becomes editable ──────────────────────────────────────────

// The English of the two read-only sentences the page no longer shows: 2h.1's
// 130 removed `model-profile-read-only` and 2h.10's 194 removed
// `model-profile-partly-editable`. Both keys are out of the strings file (pinned by
// App.test.tsx's 1a), so the test names the text itself.
const READ_ONLY_ENGLISH =
  "Read-only for now: this page cannot change a model profile's settings yet. Change them in pai-config on the server machine."
const PARTLY_EDITABLE_ENGLISH =
  'Some settings cannot be changed here yet: their controls are greyed out. Change those in pai-config on the server machine.'

test('2h.130 (2h.10 194): no read-only Notice on the page: model-profile-read-only went in 2h.1 and model-profile-partly-editable follows it in 2h.10, so a page with an available engine shows no bordered Notice at all', async () => {
  const { main } = await page({})
  expect(
    main.querySelectorAll('.notice').length,
    '194: no Notice, the engine is available',
  ).toBe(0)
  expect(
    main.textContent,
    '2h.1: the read-only string is gone from the page',
  ).not.toContain(READ_ONLY_ENGLISH)
  expect(
    main.textContent,
    '194: the partly-editable string is gone from the page',
  ).not.toContain(PARTLY_EDITABLE_ENGLISH)
})

// ── 131: the revision ───────────────────────────────────────────────────

test('2h.131: a 200 read with no revision, a revision that is not a string, a number, null, shows model-profile-load-failed and no card', async () => {
  for (const [what, extra] of [
    ['absent', { revision: undefined }],
    ['null', { revision: null }],
    ['a number', { revision: 42 }],
    ['an object', { revision: { n: 1 } }],
  ] as const) {
    const { main } = open({
      modelProfile: () =>
        reply(200, modelProfileDoc({}, { values: {}, ...extra })),
    })
    const shown = await within(main).findByText(en('model-profile-load-failed'))
    expect(shown, `revision ${what}: the message`).toBeTruthy()
    expect(
      within(main).queryAllByRole('heading', { level: 3 }).length,
      `revision ${what}: no card`,
    ).toBe(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2h.131: the save sends the read revision back untouched: the fe never parses it', async () => {
  const opaque = 'R 9/8-∅ x'
  const { bff, main } = await loaded({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { values: { pooling: 'mean' }, revision: opaque }),
      ),
  })
  pick(main, POOLING, 'cls')
  await save(main, bff)
  expect(sentBody(bff).revision, 'the same string, as read').toBe(opaque)
})

// ── 132: changed and dirty ──────────────────────────────────────────────

test('2h.132: a fresh page is clean: the status is empty and both bar buttons are inert; a changed Embedding mode makes it dirty, and back to the loaded state makes it clean again', async () => {
  const { main } = await page({ embeddings: 'true', pooling: 'mean' })
  const line = status(main)
  expect(line.textContent, 'clean on load').toBe('')
  expect(inactive(saveButton(main)), 'Save inert while clean').toBe(true)
  expect(inactive(discardButton(main)), 'Discard inert while clean').toBe(true)

  pick(main, MODE, 'false')
  expect(line.textContent, 'dirty').toBe(en('save-unsaved'))
  expect(inactive(saveButton(main)), 'Save live when dirty').toBe(false)
  expect(inactive(discardButton(main)), 'Discard live when dirty').toBe(false)

  // D-b: back to the loaded state, not to what was typed.
  pick(main, MODE, 'true')
  expect(line.textContent, 'clean again').toBe('')
  expect(inactive(saveButton(main)), 'Save inert again').toBe(true)
})

test('2h.132: a Pooling change to the same option it loaded in is not dirty, and an option change is; a hint toggle changes nothing', async () => {
  const { bff, main } = await page({ pooling: 'mean' })
  const line = status(main)
  pick(main, POOLING, 'mean')
  expect(line.textContent, 'the same option is not a change').toBe('')
  pick(main, POOLING, 'none')
  expect(line.textContent, 'another option is a change').toBe(
    en('save-unsaved'),
  )
  pick(main, POOLING, 'mean')
  expect(line.textContent, 'back to the loaded option').toBe('')

  // 132: hint toggles change nothing (the About of an editable and of a
  // disabled field). 22 IP5.5 leaves the page's disabled rows in the Vision
  // card: its numbers need a vision file this read does not name.
  for (const label of [MODE, en('model-profile-min-image-tokens')]) {
    fireEvent.click(
      within(main).getByRole('button', {
        name: `${en('field-about')} ${label}`,
      }),
    )
    expect(line.textContent, `About ${label}: the form stays clean`).toBe('')
  }
  expect(bff.to('PUT', PUT).length, 'a hint toggle sends nothing').toBe(0)
})

// ── 132a: the extra option of the loaded value stays listed ─────────────

test('2h.132a: the extra option of an unlisted loaded Pooling value stays last in the list whatever is picked, and picking it again leaves the form clean: a save sends nothing', async () => {
  const { bff, main } = await page({ pooling: WEIRD })
  expect(
    optionValues(main, POOLING).at(-1),
    'the extra option is last on load',
  ).toBe(WEIRD)

  // Pick a listed option: the loaded state stays reachable from the control.
  pick(main, POOLING, 'mean')
  expect(
    optionValues(main, POOLING),
    'the row options, plus the extra option still last',
  ).toEqual([...POOL_LISTED, WEIRD])
  expect(selectOf(main, POOLING).value, 'mean is what is picked now').toBe(
    'mean',
  )

  // Pick the extra option again: the field is back to the state the read gave,
  // so the form is clean and a click on Save sends nothing (132, 133).
  pick(main, POOLING, WEIRD)
  expect(
    selectOf(main, POOLING).value,
    'the extra option is picked again',
  ).toBe(WEIRD)
  expect(status(main).textContent, 'back to the loaded value: clean').toBe('')
  fireEvent.click(saveButton(main))
  await act(async () => {})
  expect(bff.to('PUT', PUT), 'a clean form sends nothing').toHaveLength(0)
})

test('2h.132a: a 200 that makes the loaded value one the select lists drops the extra option, and a select whose loaded value is listed never gains one', async () => {
  const { bff, main } = await page({ pooling: WEIRD })
  pick(main, POOLING, 'mean')
  await save(main, bff)
  await waitFor(() =>
    expect(status(main).textContent, 'the 200 of the default fake').toBe(
      en('save-saved'),
    ),
  )
  expect(
    optionValues(main, POOLING),
    'the loaded value is listed now: the extra option is gone',
  ).toEqual(POOL_LISTED)

  // A loaded value the row lists: no extra option on load, and none appears
  // when the user picks another listed option.
  cleanup()
  vi.unstubAllGlobals()
  const second = await page({ pooling: 'mean' })
  expect(
    optionValues(second.main, POOLING),
    'loaded mean: the row list alone',
  ).toEqual(POOL_LISTED)
  pick(second.main, POOLING, 'cls')
  expect(
    optionValues(second.main, POOLING),
    'picking cls gains no extra option',
  ).toEqual(POOL_LISTED)
})

// ── 133: what a changed field sends ─────────────────────────────────────

test('2h.133: Embedding mode off to on sends "true" as a string, alone in values, and the body is only revision and values', async () => {
  const { bff, main } = await page({ embeddings: 'false', pooling: 'mean' })
  pick(main, MODE, 'true')
  await save(main, bff)
  const body = sentBody(bff)
  expect(
    Object.keys(body).sort(),
    'the body of the contract: revision and values',
  ).toEqual(['revision', 'values'])
  expect(sentValues(bff).embeddings, 'on sends the string "true"').toBe('true')
  expect(
    Object.keys(sentValues(bff)),
    'the unchanged Pooling key is absent',
  ).toEqual(['embeddings'])
})

test('2h.133: Embedding mode on to off sends null, the GUI spelling of a switch that is off', async () => {
  const { bff, main } = await page({ embeddings: 'true' })
  pick(main, MODE, 'false')
  await save(main, bff)
  expect(sentValues(bff).embeddings, 'off removes the key').toBeNull()
  expect(Object.keys(sentValues(bff))).toEqual(['embeddings'])
})

test('2h.133: Pooling default to an option sends the option value; an option back to default sends null', async () => {
  const first = await page({})
  pick(first.main, POOLING, 'rank')
  await save(first.main, first.bff)
  expect(sentValues(first.bff).pooling, 'the option value').toBe('rank')
  expect(Object.keys(sentValues(first.bff))).toEqual(['pooling'])

  cleanup()
  vi.unstubAllGlobals()
  const second = await page({ pooling: 'last' })
  pick(second.main, POOLING, 'default')
  await save(second.main, second.bff)
  expect(sentValues(second.bff).pooling, 'default removes the key').toBeNull()
})

test('2h.133: an unlisted Pooling value is the loaded state: picking the extra option again sends nothing, picking a listed option sends its value', async () => {
  const { bff, main } = await page({
    embeddings: 'false',
    pooling: EXTRA_POOL,
  })
  const select = selectOf(main, POOLING)
  expect(select.value, 'the extra option is selected').toBe(EXTRA_POOL)

  // Pick a listed option: sent.
  pick(main, POOLING, 'last')
  await save(main, bff)
  expect(sentValues(bff).pooling, 'a listed option sends its value').toBe(
    'last',
  )

  // Back to the extra option: the field is unchanged, so a save of another
  // field must not carry pooling.
  pick(main, POOLING, EXTRA_POOL)
  pick(main, MODE, 'true')
  await save(main, bff, 2)
  expect(
    Object.keys(sentValues(bff, 2)),
    'the extra option is never sent',
  ).toEqual(['embeddings'])
})

test('2h.133: a field changed back to its loaded state is absent from the body, and an untouched page sends nothing', async () => {
  const { bff, main } = await page({ embeddings: 'true', pooling: 'mean' })
  pick(main, MODE, 'false')
  pick(main, MODE, 'true')
  pick(main, POOLING, 'cls')
  pick(main, POOLING, 'mean')
  expect(status(main).textContent, 'nothing changed: clean').toBe('')
  // The Save button is inert while clean (135), so no PUT can go out.
  expect(bff.to('PUT', PUT).length, 'a clean form sends nothing').toBe(0)
})

// ── 134: the lines under a control while editing ────────────────────────

test('2h.134: the written-as line of Embedding mode shows while the field is unchanged, hides when it is changed, and the control drops its id', async () => {
  const { main } = await page({ embeddings: '0', pooling: 'mean' })
  const select = selectOf(main, MODE)
  const line = writtenOf(main, 'embeddings')
  expect(line, 'the line of 124 for the file spelling "0"').toBeTruthy()
  expect(text(line!), 'the file spelling').toBe(
    en('model-profile-written-as', '0'),
  )
  expect(describedIds(select), 'the control names the line (124b)').toContain(
    'model-profile-written-embeddings',
  )
  expect(isHidden(line!), 'shown').toBe(false)

  pick(main, MODE, 'true')
  expect(
    isHidden(writtenOf(main, 'embeddings')),
    'a changed field hides the line',
  ).toBe(true)
  expect(
    describedIds(selectOf(main, MODE)),
    'and the control drops its id',
  ).not.toContain('model-profile-written-embeddings')

  pick(main, MODE, 'false')
  expect(
    isHidden(writtenOf(main, 'embeddings')),
    'the file spelling shows again once the field is back off',
  ).toBe(false)
})

// ── 139 with 138: an edit made while the PUT runs, and the save fails ───

test('2h.139, 2h.138: an edit made while the PUT runs survives a failed save: both edits stay in the controls, the form is dirty with the save-failed alert, and the next Save sends both keys with the revision that was read', async () => {
  const gate = deferred()
  const { bff, main } = await form(
    { embeddings: 'true', pooling: 'mean' },
    (_b, _id, n) =>
      n === 1 ? gate.promise : reply(200, { revision: 'r2', running: false }),
  )
  // First edit, then the save; the PUT is held pending.
  pick(main, MODE, 'false')
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  // Second edit, made while the PUT runs (137: the fields stay editable).
  pick(main, POOLING, 'rank')
  await act(async () => {
    gate.resolve(reply(500, { key: 'secret-detail', args: [] }))
  })
  await within(main).findByText(en('save-failed'))
  expectAlert(main, en('save-failed'))
  expect(
    document.body.textContent,
    'the manager words never reach the page',
  ).not.toContain('secret-detail')
  expect(
    selectOf(main, MODE).value,
    'the edit made before the save stays',
  ).toBe('false')
  expect(
    selectOf(main, POOLING).value,
    'the edit made while it runs stays too',
  ).toBe('rank')
  expect(status(main).textContent, 'the form is dirty').toBe(en('save-unsaved'))
  expect(inactive(saveButton(main)), 'Save is live again').toBe(false)
  // The next save carries both edits and the revision of the read: a failed
  // save leaves the loaded state alone (139).
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'both keys, the read revision').toEqual({
    revision: 'r1',
    values: { embeddings: null, pooling: 'rank' },
  })
})
