// Part of sub-slice 2h.5 of fe/briefs/02h-model-profile-save.md, as brief 22
// IP5 narrows it: the KV cache card is cache-reuse alone (the cache types,
// flash-attn, Cache RAM and the context-checkpoint pair move to the instance
// profile with the rest of `PLACEMENT_KEYS`), kept editable (164: its field
// always enabled, its number box ticked for a key the file lacks) and integer
// checked (157: cache-reuse 1 or more). The Embeddings row of 133 stays too.
// The card's read-only look is 2e's 96, in ModelProfileCards2.test.tsx; the
// helpers are those of ModelProfileSave7.test.tsx, which this file repeats (a
// test file imports no other test file).
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
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const REUSE = en('model-profile-cache-reuse')
const DEFAULT = en('field-default')
/** The card's number fields whose input sits behind a default box (156), by
 * key, with the label. As IP5 narrows the card, cache-reuse is the only one. */
const BOXED: Record<string, string> = {
  'cache-reuse': REUSE,
}
/** What the read gives (2e's 99, the card's keys as IP5 leaves them). */
const AS_LOADED: Record<string, string> = {
  'cache-reuse': '256',
}
/** The card's fields, as 2e's table 96 leaves them (IP5.5): cache-reuse. */
const FIELDS: { key: string; label: string }[] = Object.entries(BOXED).map(
  ([key, label]) => ({ key, label }),
)

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as ModelProfileSave6 and ModelProfileSave7.test.tsx) ───────────────────

/** Waits for the page read with revision `r1` and the card's keys as `values`. */
async function form(
  values: Record<string, string> = AS_LOADED,
  options: BffOptions = {},
) {
  const bff = fakeBff({
    ...options,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The field whose label is `label` (its group). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The number input of a field, by its accessible name. */
function box(main: HTMLElement, label: string): HTMLInputElement {
  return within(field(main, label)).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

/** The field's default box. */
function checkBox(main: HTMLElement, label: string): HTMLInputElement {
  return within(field(main, label)).getByRole('checkbox', {
    name: DEFAULT,
  }) as HTMLInputElement
}

/** Focuses and clicks the default box, as a pointer or Enter does. */
function toggle(main: HTMLElement, label: string): HTMLInputElement {
  const c = checkBox(main, label)
  c.focus()
  fireEvent.click(c)
  return c
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
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

/** The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line and before the buttons (135, 01's 30a). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(
    before(alert!, discardButton(main)),
    `${what}: before the buttons`,
  ).toBe(true)
}

/** The ids of `aria-describedby`, in order. */
function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

/** The text each id of `aria-describedby` gives. */
function described(el: HTMLElement): string[] {
  return describedIds(el).map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 157). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = describedIds(el)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

// ── 164: every field of the card is enabled, always ─────────────────────

test.each(FIELDS)(
  '2h.164: the card field $key has its control enabled always, and a number field has its default box enabled: ticked for a key the file lacks, unticked for one it has',
  async (n) => {
    const { main } = await form()
    const present = n.key in AS_LOADED
    if (n.key in BOXED) {
      const c = checkBox(main, n.label)
      expect(c.disabled, `${n.key}: the box enabled`).toBe(false)
      expect(c.checked, `${n.key}: the box ticked when the key is absent`).toBe(
        !present,
      )
      const input = box(main, n.label)
      expect(input.disabled, `${n.key}: the input follows the box`).toBe(
        !present,
      )
      expect(input.value, `${n.key}: the input holds the file's text`).toBe(
        present ? AS_LOADED[n.key] : '',
      )
      return
    }
    expect(
      selectOf(main, n.label).disabled,
      `${n.key}: the select enabled`,
    ).toBe(false)
  },
)

// ── 133: what the two cache-type selects send ───────────────────────────
// The cache types, flash-attn, Cache RAM and the checkpoint pair left the card
// with `PLACEMENT_KEYS` (brief 22 IP5.5): the section's selects, tri-state and
// Cache RAM unit tests leave with them.

// ── 157: the integer check of the card ──────────────────────────────────

/** Texts that fail their field (157): cache-reuse's range, 1 or more. */
const BAD: { key: string; label: string; text: string; error: string }[] = [
  {
    key: 'cache-reuse',
    label: REUSE,
    text: '0',
    error: en('field-error-number-min', '1'),
  },
]

test.each(BAD)(
  '2h.157: $key rejects "$text" with $error, marks it, focuses it, alerts save-invalid and sends nothing',
  async (c) => {
    const { bff, main } = await form()
    if (!(c.key in AS_LOADED)) toggle(main, c.label)
    type(box(main, c.label), c.text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = box(main, c.label)
    expectError(el, c.error, c.label)
    expect(document.activeElement, `focus on ${c.label}`).toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

// ── 132, 133: a key the card lacks shows its absent option ───────────────
// The card's three selects left with `PLACEMENT_KEYS` (IP5.5), and the flag
// rows of 132 with them: nothing is left on this one-number card to pin.

// ── 133: embeddings = false in the file, on then off again ────────────────
// The off of a key the file holds is the loaded state, so it is not a change
// and the save sends nothing at all: `embeddings: null` would remove the line
// the user never asked to remove (132, 133's off / on row).

test('2h.133: embeddings = false in the file: untouched clean, on dirty, off again clean with no PUT and no embeddings: null', async () => {
  const { bff, main } = await form({ ...AS_LOADED, embeddings: 'false' })
  expect(selectOf(main, MODE).value, 'the file spells off').toBe('false')
  expect(status(main).textContent, 'untouched: clean').toBe('')
  pick(main, MODE, 'true')
  expect(status(main).textContent, 'on: dirty').toBe(en('save-unsaved'))
  expect(
    saveButton(main).getAttribute('aria-disabled') ?? 'false',
    'dirty: Save enabled',
  ).toBe('false')
  pick(main, MODE, 'false')
  expect(status(main).textContent, 'off again: clean').toBe('')
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), 'clean: the buttons inert').toBe(
      'true',
    )
  fireEvent.click(saveButton(main))
  expect(
    bff.to('PUT', PUT),
    'no PUT, so no embeddings: null either',
  ).toHaveLength(0)
})
