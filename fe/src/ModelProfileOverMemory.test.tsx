// The two resource refusals of a model profile, on the profile page
// (fe/briefs/11-resource-refusals.md, slice M1): a save answered 422
// model-profile-over-memory (11 M1.2 to 11 M1.5) and 422
// model-profile-model-missing (11 M1.11, 11 M1.12). The bar's alert says the
// key's text inside save-refused, no field is marked, the focus stays on Save
// changes, the edits stay, and the api-other sentence is gone. The strings
// themselves (11 M1.1, 11 M1.10) are strings.test.ts, and the Models list
// (11 M1.6 to 11 M1.9, 11 M1.13) is ModelProfilesOverMemory.test.tsx. The bar,
// the PUT and the other answers are 2h's, in ModelProfileSave1 to 16.test.tsx,
// whose helpers this file repeats (a test file imports no other test file).
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
import {
  before,
  en,
  fakeBff,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

/** The fixture of the brief (section 4): the profile page of `qwen-27b`. */
const ID = 'qwen-27b'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const CTX = en('model-profile-context-size')
/** The Context size typed before saving: the fix the refusal's text names. */
const CTX_TO = '8192'
/** The two refusals, with the brief's args and, for the missing model file,
 * the `field` the bff's `field_of` maps it to (`model`: the read-only Assets
 * row, so 02h's 143: the bar's alert only). */
const MEMORY = {
  key: 'model-profile-over-memory',
  args: ['qwen-27b', 'default', '30721', '24560'],
}
const MISSING = {
  key: 'model-profile-model-missing',
  args: ['qwen-27b', 'default', '/srv/models/qwen-27b.gguf'],
  field: 'model',
}
/** What the read gives: Context size set, so its default box starts unticked
 * (156) and its input is the one the user can type in. */
const AS_LOADED = {
  model: '/srv/models/qwen-27b.gguf',
  'ctx-size': '172032',
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileSave3/6.test.tsx) ───────────────────────────────

/** Waits for the loaded page of `ID` with revision `r1` and `AS_LOADED`. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { id: ID, revision: 'r1', values: AS_LOADED }),
      ),
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-resource-context'),
  })
  return { bff, main }
}

/** The field whose label is `label` (its group). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The number input of a field. */
function inputOf(main: HTMLElement, label: string): HTMLInputElement {
  return within(field(main, label)).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-button'),
  }) as HTMLButtonElement
}

function discardButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-discard'),
  }) as HTMLButtonElement
}

/** The save bar's status line (the only role="status" in the settings). */
function settingsHeading(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
}

function status(main: HTMLElement): HTMLElement {
  const h2 = settingsHeading(main)
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
}

/** The save bar's alerts: the role="alert" elements below the settings
 * heading, where the bar is (135). */
function barAlerts(main: HTMLElement): HTMLElement[] {
  const h2 = settingsHeading(main)
  return within(main)
    .queryAllByRole('alert')
    .filter((a) => before(h2, a))
}

/** The page, dirty (Context size `CTX_TO`), with the save answered 422 `body`,
 * and its alert rendered. */
async function refused(body: Record<string, unknown>) {
  const { bff, main } = await loaded({
    modelProfileSave: () => reply(422, body),
  })
  fireEvent.change(inputOf(main, CTX), { target: { value: CTX_TO } })
  saveButton(main).focus()
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  await waitFor(() =>
    expect(barAlerts(main), 'the bar answered').toHaveLength(1),
  )
  return { bff, main }
}

/** The bar's one alert, as text. */
function alertText(main: HTMLElement): string {
  const all = barAlerts(main)
  expect(all, 'one alert in the bar').toHaveLength(1)
  return all[0]!.textContent ?? ''
}

/** The text of a refusal: the key's English, in the bar's "Not saved: {0}". */
function refusedText(refusal: { key: string; args: string[] }): string {
  return en('save-refused', en(`api-${refusal.key}`, ...refusal.args))
}

/** No field is marked: no control is invalid, and no field error text shows. */
function expectNothingMarked(main: HTMLElement, what: string) {
  const invalid = [...main.querySelectorAll('[aria-invalid="true"]')]
  expect(
    invalid.map((e) => e.outerHTML),
    `${what}: no control aria-invalid`,
  ).toEqual([])
  const errors = [...main.querySelectorAll('.field-error')]
    .map((e) => (e.textContent ?? '').trim())
    .filter((text) => text !== '')
  expect(errors, `${what}: no field error text`).toEqual([])
}

// ── The memory refusal (11 M1.2 to 11 M1.5) ────────────────────────────────

test('11 M1.2: a save refused model-profile-over-memory says the GUI sentence and the next step in the bar alert, with both MiB figures as sent', async () => {
  const { main } = await refused(MEMORY)
  const text = refusedText(MEMORY)
  const el = await within(main).findByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, 'the refusal is in role="alert"').not.toBe(null)
  expect(alert!.textContent, 'the bar alert says only the refusal').toBe(text)
  expect(text, 'the needed MiB as sent').toContain('it needs 30721 MiB')
  expect(text, 'the available MiB as sent').toContain('the node has 24560 MiB')
  expect(
    alert!.textContent ?? '',
    'no thousands separator, no GiB: the figures as the bff sent them',
  ).not.toMatch(/30[\s,.]721|24[\s,.]560|GiB/)
})

test('11 M1.3: after the over-memory refusal no field shows an error text and no control has aria-invalid', async () => {
  // A guard: 02h's 143 already answers a key with no field this way. The bar
  // alert of `refused` is the page's answer, whatever its text.
  const { main } = await refused(MEMORY)
  expectNothingMarked(main, 'the over-memory refusal')
  const ctx = inputOf(main, CTX)
  expect(ctx.getAttribute('aria-invalid') ?? 'false', 'Context size').toBe(
    'false',
  )
})

test('11 M1.4: after the over-memory refusal the focus is on Save changes, the typed Context size stays, and Save changes and Discard stay enabled', async () => {
  // A guard (02h's 144 and 151 already do this for a key with no field).
  const { main } = await refused(MEMORY)
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  expect(inputOf(main, CTX).value, 'the edit stays').toBe(CTX_TO)
  expect(saveButton(main).disabled, 'Save changes enabled').toBe(false)
  expect(discardButton(main).disabled, 'Discard enabled').toBe(false)
  expect(status(main).textContent, 'still dirty').toBe(en('save-unsaved'))
})

test('11 M1.5: after the over-memory refusal the api-other sentence is nowhere on the page', async () => {
  const { main } = await refused(MEMORY)
  expect(
    within(main).queryByText(en('api-other', MEMORY.key)),
    'no api-other element',
  ).toBeNull()
  expect(
    document.body.textContent ?? '',
    'the bar says the new text, not the fallback',
  ).not.toContain('The request was refused')
  expect(alertText(main), 'the bar alert').toBe(refusedText(MEMORY))
})

// ── The missing model file (11 M1.11, 11 M1.12) ────────────────────────────

test('11 M1.11: a save refused model-profile-model-missing says the profile, the path, the instance and the next step in the bar alert, and no api-other', async () => {
  const { main } = await refused(MISSING)
  const text = refusedText(MISSING)
  const el = await within(main).findByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, 'the refusal is in role="alert"').not.toBe(null)
  expect(alert!.textContent, 'the bar alert says only the refusal').toBe(text)
  expect(text, 'the path as sent').toContain('`/srv/models/qwen-27b.gguf`')
  expect(
    within(main).queryByText(en('api-other', MISSING.key)),
    'no api-other element',
  ).toBeNull()
  expect(
    document.body.textContent ?? '',
    'the bar says the new text, not the fallback',
  ).not.toContain('The request was refused')
})

test('11 M1.12: after the model-missing refusal no field is marked, the focus is on Save changes, and Save changes and Discard stay enabled', async () => {
  const { main } = await refused(MISSING)
  // 02h's 143: the field the reply names (`model`) is the read-only Model file
  // row, with no control, so the bar's alert is all there is. A guard.
  expectNothingMarked(main, 'the model-missing refusal')
  const at = en(
    'save-refused-at',
    en('api-model-profile-model-missing', ...MISSING.args),
    en('model-profile-model-file'),
    en('model-profile-assets'),
  )
  expect(alertText(main), 'the plain bar alert, not 198a').toBe(
    refusedText(MISSING),
  )
  expect(alertText(main), 'the field has no control at all').not.toBe(at)
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  expect(inputOf(main, CTX).value, 'the edit stays').toBe(CTX_TO)
  expect(saveButton(main).disabled, 'Save changes enabled').toBe(false)
  expect(discardButton(main).disabled, 'Discard enabled').toBe(false)
})
