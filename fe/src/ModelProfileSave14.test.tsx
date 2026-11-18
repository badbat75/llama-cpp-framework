// The Fit trio of the Hardware card editable, sub-slice 2h.8 of
// fe/briefs/02h-model-profile-save.md (183, 184, 185, 186, 187, 188): Fit to VRAM
// always enabled with the checked state of 2e.3c's 127, its toggle text as the
// table of 184 (and the text that follows an untick of Context size's default
// box, silently), an invalid `fit` of 2e's 128 that blocks the Save, what a
// checked and an unchecked box send (133), the two dependents enabled while
// the box is checked (186), the min-context range of 157, the Fit free VRAM
// list of MiB of `checks::fit_target` with `field-error-fit-target` and its
// unit, the locked dependents checked at Save when their draft differs from
// the loaded value and skipped when it is the loaded value (176's "what the
// page sends, it checks", 2026-11-18), and the four new strings. The new 2h.7's
// 182a of the layer counts (the refusal of GPU layers and Draft GPU layers
// names `auto` and `all`) is here too, beside 157's accept of the words.
// The card's other rows are 2h.7's 175 to 182, in ModelProfileSave13.test.tsx, whose
// helpers this file repeats (a test file imports no other test file).
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
import { t } from './strings.ts'
import {
  before,
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
const FIT = en('model-profile-fit-to-vram')
const FIT_CTX = en('model-profile-fit-min-context')
const FIT_TARGET = en('model-profile-fit-free-vram')
const CTX = en('model-profile-context-size')
const OP = en('model-profile-op-offload')
const GPU_LAYERS = en('model-profile-gpu-layers')
const DRAFT_LAYERS = en('model-profile-draft-gpu-layers')
const FFN = en('model-profile-dense-ffn-cpu-layers')
const DEFAULT = en('field-default')
/** The five toggle texts of 184's table. */
const ALLOWED = en('model-profile-fit-allowed')
const NOT_ALLOWED = en('model-profile-fit-not-allowed')
const LAYERS_ONLY = en('model-profile-fit-layers-only')
const CONTEXT_ONLY = en('model-profile-fit-context-only')
const NOTHING = en('model-profile-fit-nothing')
/** 128, 129: the error of a value llama-server refuses. */
const FIT_ERR = en('model-profile-fit-invalid')
/** 157's `fit-ctx` row: 1 or more. */
const MIN_ONE = en('field-error-number-min', '1')
/** 186: the two texts of the Fit free VRAM check. */
const MARKERS = en('field-error-comment-markers')
const TARGET_ERR = en('field-error-fit-target')
/** 182a: the words the two layer counts take, and the general text a key that
 * takes no word keeps. */
const GPU_ERR = en('field-error-gpu-layers')
const WHOLE = en('field-error-whole-number')
/** The literal unit of 107's Fit free VRAM row (89, 159). */
const MIB = 'MiB'
/** 124: the written-as line's texts. */
const as_ = (value: string) => en('model-profile-written-as', value)
const EMPTY = en('model-profile-written-empty')

/** A read of the card: the Fit trio's three keys plus a row the tests change,
 * with Context size and GPU layers at default (the key absent) or set. */
function loaded(fit: string | undefined, ctx: boolean, layers: boolean) {
  const values: Record<string, string> = {
    'fit-ctx': '4096',
    'fit-target': '2048',
    'op-offload': 'true',
  }
  if (fit !== undefined) values.fit = fit
  if (ctx) values['ctx-size'] = '8192'
  if (layers) values['n-gpu-layers'] = '99'
  return values
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileSave13.test.tsx) ─────────────────────────────────

/** Waits for the page read with revision `r1` and the card's keys as `values`. */
async function form(
  values: Record<string, string> = loaded('on', false, false),
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
    name: en('model-profile-hardware-config'),
  })
  return { bff, main }
}

/** The field whose label is `label` (its group). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The number or text input of a field, by its accessible name. */
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

/** Fit to VRAM's own checkbox: the field holds one, named by its toggle text,
 * which 184 picks, so it is taken by its position, not by a name. */
function fitBox(main: HTMLElement): HTMLInputElement {
  return within(field(main, FIT)).getByRole('checkbox') as HTMLInputElement
}

/** The checkbox that 184's table gives the text `text`, as the accessible name
 * of Fit to VRAM's box. */
function fitNamed(main: HTMLElement, text: string): HTMLInputElement {
  return within(field(main, FIT)).getByRole('checkbox', {
    name: text,
  }) as HTMLInputElement
}

/** A text field's input, by its accessible name: a plain text row is no
 * labelled group (a number row is, for its default box). */
function inputOf(main: HTMLElement, label: string): HTMLInputElement {
  return within(main).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** The save bar's two buttons are inert: the form is clean. */
function expectInert(main: HTMLElement, what: string) {
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), `${what}: the buttons inert`).toBe(
      'true',
    )
}

/** The save bar's two buttons are live: the form is dirty (132). */
function expectLive(main: HTMLElement, what: string) {
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), `${what}: the buttons live`).toBe(
      null,
    )
}

/** The save bar's status line (the only role="status" below the h2 of the
 * settings, as ModelProfileSave1.test.tsx takes it). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The `values` of the n-th recorded PUT of this model profile. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
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

/** Asserts `el` carries no mark and no error text (the field is clean). */
function expectNoError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what}: no mark`).toBe(null)
  expect(described(el), `${what}: no error`).not.toContain(text)
}

/** The save bar's alert of 01's 31. */
function expectAlert(main: HTMLElement, text: string) {
  const el = within(main).getByText(text)
  expect(el.closest('[role="alert"]'), `${text} is in role="alert"`).not.toBe(
    null,
  )
}

/** The unit of an input: the id 159 gives it. */
function unitId(el: HTMLElement): string {
  return `${el.id.replace(/-control$/, '')}-unit`
}

// ── 183, 127, 184, 186: the box, its text, its dependents at load ─────

/** One read of the card and what the trio shows from it. */
type Load = {
  name: string
  values: Record<string, string>
  checked: boolean
  text: string
  /** The written-as line's text, or `null` for no line (127). */
  written: string | null
  /** The field error's text, or `null` (128). */
  error: string | null
}

const LOADS: Load[] = [
  {
    name: 'fit absent',
    values: loaded(undefined, false, false),
    checked: true,
    text: ALLOWED,
    written: null,
    error: null,
  },
  {
    name: 'fit off',
    values: loaded('off', false, false),
    checked: false,
    text: ALLOWED,
    written: null,
    error: null,
  },
  {
    name: 'fit TRUE',
    values: loaded('TRUE', false, false),
    checked: false,
    text: ALLOWED,
    written: as_('TRUE'),
    error: FIT_ERR,
  },
  {
    name: 'fit empty',
    values: loaded('', false, false),
    checked: false,
    text: ALLOWED,
    written: EMPTY,
    error: FIT_ERR,
  },
  {
    name: 'fit auto',
    values: loaded('auto', false, false),
    checked: false,
    text: ALLOWED,
    written: as_('auto'),
    error: FIT_ERR,
  },
  {
    name: 'fit absent, Context size set',
    values: loaded(undefined, true, false),
    checked: true,
    text: LAYERS_ONLY,
    written: null,
    error: null,
  },
  {
    name: 'fit off, Context size set',
    values: loaded('off', true, false),
    checked: false,
    text: NOT_ALLOWED,
    written: null,
    error: null,
  },
  {
    name: 'fit off, GPU layers set',
    values: loaded('off', false, true),
    checked: false,
    text: NOT_ALLOWED,
    written: null,
    error: null,
  },
  {
    name: 'fit off, both set',
    values: loaded('off', true, true),
    checked: false,
    text: NOT_ALLOWED,
    written: null,
    error: null,
  },
  {
    name: 'fit TRUE, GPU layers set',
    values: loaded('TRUE', false, true),
    checked: false,
    text: NOT_ALLOWED,
    written: as_('TRUE'),
    error: FIT_ERR,
  },
  {
    name: 'fit on, Context size set',
    values: loaded('on', true, false),
    checked: true,
    text: LAYERS_ONLY,
    written: null,
    error: null,
  },
  {
    name: 'fit on, GPU layers set',
    values: loaded('on', false, true),
    checked: true,
    text: CONTEXT_ONLY,
    written: null,
    error: null,
  },
  {
    name: 'fit on, both set',
    values: loaded('on', true, true),
    checked: true,
    text: NOTHING,
    written: null,
    error: null,
  },
]

test.each(LOADS)(
  '2h.183, 2h.127, 2h.184, 2h.186: $name: the box is enabled and $checked, the text is $text, and the two dependents follow the box',
  async (c) => {
    const { main } = await form(c.values)
    // 183: always enabled, whatever Context size, GPU layers and the value.
    const fit = fitBox(main)
    expect(fit.disabled, `${c.name}: Fit to VRAM is enabled (183)`).toBe(false)
    // 127: the box shows what llama-server does.
    expect(fit.checked, `${c.name}: 127: the box`).toBe(c.checked)
    // 184: the toggle text is the box's accessible name.
    expect(fitNamed(main, c.text), `${c.name}: 184: the toggle text`).toBe(fit)
    // 124, 127, 128: the line and the error under the box.
    const line = document.getElementById('model-profile-written-fit')
    if (c.written === null)
      expect(line, `${c.name}: no written-as line (127)`).toBeNull()
    else
      expect(line!.textContent, `${c.name}: 124: the line under the box`).toBe(
        c.written,
      )
    const error = document.getElementById('model-profile-fit-error')
    if (c.error === null) {
      expect(error, `${c.name}: no error (128)`).toBeNull()
      expect(fit.getAttribute('aria-invalid'), `${c.name}: not marked`).toBe(
        null,
      )
    } else {
      expect(error!.textContent, `${c.name}: 128: the field error`).toContain(
        c.error,
      )
      expect(
        describedIds(fit),
        `${c.name}: 128: the box names the error`,
      ).toContain('model-profile-fit-error')
      expect(fit.getAttribute('aria-invalid'), `${c.name}: marked`).toBe('true')
    }
    // 186: the two dependents are enabled while the box is checked.
    for (const [el, what] of [
      [checkBox(main, FIT_CTX), 'Fit min context box'],
      [box(main, FIT_CTX), 'Fit min context input'],
      [inputOf(main, FIT_TARGET), 'Fit free VRAM input'],
    ] as [HTMLInputElement, string][])
      expect(
        el.disabled,
        `${c.name}: ${what} enabled as the box is checked (186)`,
      ).toBe(!c.checked)
    // 156: Fit min context is a boxed field: the key the file has unticks it.
    if (c.checked)
      expect(
        box(main, FIT_CTX).value,
        `${c.name}: the input holds the file text`,
      ).toBe('4096')
  },
)

test('2h.184: unticking Context size default box changes the toggle text silently, the box keeps its state and the text is no live region', async () => {
  const { main } = await form(loaded('on', false, false))
  expect(
    checkBox(main, CTX).checked,
    'Context size at default: the box ticked',
  ).toBe(true)
  const fit = fitBox(main)
  expect(fit.checked, 'fit on: checked').toBe(true)
  expect(
    fitNamed(main, ALLOWED),
    '184: both at default: model-profile-fit-allowed',
  ).toBe(fit)
  toggle(main, CTX)
  const after = fitBox(main)
  expect(after, 'the same box, not a new one').toBe(fit)
  expect(after.checked, '184: the box keeps its state').toBe(true)
  expect(
    fitNamed(main, LAYERS_ONLY),
    '184: Context size set and checked: model-profile-fit-layers-only',
  ).toBe(after)
  expect(within(main).queryByText(ALLOWED), 'the first text is gone').toBeNull()
  for (const el of within(main).getAllByText(LAYERS_ONLY)) {
    expect(el.getAttribute('aria-live'), '184: not a live region').toBeNull()
    expect(
      el.closest('[role="status"],[role="alert"],[role="log"],[aria-live]'),
      '184: not inside a live region',
    ).toBeNull()
  }
  toggle(main, CTX)
  expect(fitNamed(main, ALLOWED), '184: back to the first text').toBe(
    fitBox(main),
  )
})

// ── 185: an invalid fit ───────────────────────────────────────────────

test('2h.185: fit TRUE blocks the Save even when another field changed: the error stays, focus on the box, save-invalid, nothing sent', async () => {
  const { bff, main } = await form(loaded('TRUE', false, false))
  pick(main, OP, 'false')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const fit = fitBox(main)
  expect(fit.checked, 'the box stays unchecked').toBe(false)
  expectError(fit, FIT_ERR, 'fit TRUE at Save')
  expect(document.activeElement, '185: focus on the Fit to VRAM box').toBe(fit)
  expectAlert(main, en('save-invalid'))
  expect(bff.to('PUT', PUT), '185: nothing sent').toHaveLength(0)
})

test('2h.185: fit TRUE: one click checks it and sends on and clears the line and the error', async () => {
  const first = await form(loaded('TRUE', false, false))
  fireEvent.click(fitBox(first.main))
  const one = fitBox(first.main)
  expect(one.checked, 'the first click checks the box').toBe(true)
  expect(one.getAttribute('aria-invalid'), 'changed: not marked').toBe(null)
  expect(
    document.getElementById('model-profile-fit-error'),
    'changed: no error',
  ).toBeNull()
  expect(
    document.getElementById('model-profile-written-fit'),
    'changed: no written-as line',
  ).toBeNull()
  await save(first.main, first.bff)
  expect(sentValues(first.bff)['fit'], 'one click sends on').toBe('on')
})

test('2h.185: fit TRUE: two clicks send off', async () => {
  const second = await form(loaded('TRUE', false, false))
  fireEvent.click(fitBox(second.main))
  fireEvent.click(fitBox(second.main))
  expect(fitBox(second.main).checked, 'two clicks: unchecked').toBe(false)
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['fit'], 'two clicks send off').toBe('off')
})

test('2h.185: fit TRUE: Discard brings the line and the error back', async () => {
  const third = await form(loaded('TRUE', false, false))
  fireEvent.click(fitBox(third.main))
  fireEvent.click(discardButton(third.main))
  const back = fitBox(third.main)
  expect(back.checked, 'Discard: back to the loaded state').toBe(false)
  expectError(back, FIT_ERR, 'fit TRUE after Discard')
  expect(
    document.getElementById('model-profile-written-fit')?.textContent,
    'Discard: the written-as line is back',
  ).toBe(as_('TRUE'))
  expectInert(third.main, 'Discard: the form is clean')
})

test('2h.185, 2h.134, 2h.124, 2h.132: fit TRUE: an invalid value is a state no click returns to, so the line stays away after the second click, the field is clean and the form dirty; Discard brings line and error back', async () => {
  const { main } = await form(loaded('TRUE', false, false))
  // 124, 127, 128: at load the file's spelling is under the box, with the error.
  expect(
    document.getElementById('model-profile-written-fit')?.textContent,
    '124: at load the line shows the file spelling',
  ).toBe(as_('TRUE'))
  expectError(fitBox(main), FIT_ERR, 'fit TRUE at load')
  // The first click: the box is checked, so the state the file reads as. The
  // line goes with the state it described.
  fireEvent.click(fitBox(main))
  const one = fitBox(main)
  expect(one.checked, 'the first click checks the box').toBe(true)
  expect(
    document.getElementById('model-profile-written-fit'),
    '134: the first click drops the line',
  ).toBeNull()
  expectNoError(one, FIT_ERR, 'fit TRUE clicked to checked')
  // The second click: unchecked, so it would send `off`. An `off` is not the
  // invalid state the file holds, so the line stays away (185), the field
  // carries no mark, and the box sits where an off value puts it, which IS a
  // change: the form is dirty.
  fireEvent.click(fitBox(main))
  const two = fitBox(main)
  expect(two.checked, 'the second click unchecks it (it sends off)').toBe(false)
  expect(
    document.getElementById('model-profile-written-fit'),
    '185: the line stays away: an off is not the invalid state the file holds',
  ).toBeNull()
  expect(
    document.getElementById('model-profile-fit-error'),
    '185: no error after the second click',
  ).toBeNull()
  expectNoError(two, FIT_ERR, 'fit TRUE after two clicks')
  expect(
    status(main).textContent,
    '132: a box at an off over an invalid value is a change: the form is dirty',
  ).toBe(en('save-unsaved'))
  expectLive(main, 'fit TRUE after two clicks')
  // Discard: back to the value the read gave, so the state of load returns,
  // line and error with it, and the form is clean.
  fireEvent.click(discardButton(main))
  const back = fitBox(main)
  expect(back.checked, 'Discard: back to the loaded state').toBe(false)
  expectError(back, FIT_ERR, 'fit TRUE after Discard')
  expect(
    document.getElementById('model-profile-written-fit')?.textContent,
    '134: Discard brings the line back with the loaded state',
  ).toBe(as_('TRUE'))
  expectInert(main, 'Discard: the form is clean')
})

/** 133: a valid value, checked sends `on` and unchecked sends `off`. */
const FIT_SENDS: { name: string; fit: string | undefined; sends: string }[] = [
  { name: 'fit on, checked', fit: 'on', sends: 'off' },
  { name: 'fit off, unchecked', fit: 'off', sends: 'on' },
  { name: 'fit absent, checked', fit: undefined, sends: 'off' },
  { name: 'fit true (alias), checked', fit: 'true', sends: 'off' },
  { name: 'fit empty (invalid), unchecked', fit: '', sends: 'on' },
]

test.each(FIT_SENDS)(
  '2h.185, 2h.133: $name: one click sends $sends and the form is clean',
  async (c) => {
    const { bff, main } = await form(loaded(c.fit, false, false))
    fireEvent.click(fitBox(main))
    const el = fitBox(main)
    expectNoError(el, FIT_ERR, `${c.name}: the click is no error`)
    await save(main, bff)
    expect(sentValues(bff)['fit'], `${c.name}: sends ${c.sends}`).toBe(c.sends)
    expect(
      within(main).queryByText(en('save-invalid')),
      `${c.name}: the save is not blocked`,
    ).toBeNull()
  },
)

// ── 186: the two dependents ───────────────────────────────────────────

test('2h.186: unchecking Fit to VRAM disables Fit min context box and input and Fit free VRAM, live, and checking it enables them again', async () => {
  const { main } = await form(loaded('on', true, false))
  const dependents = [
    checkBox(main, FIT_CTX),
    box(main, FIT_CTX),
    inputOf(main, FIT_TARGET),
  ]
  for (const el of dependents)
    expect(el.disabled, 'checked: the dependent is enabled (186)').toBe(false)
  fireEvent.click(fitBox(main))
  for (const el of [
    checkBox(main, FIT_CTX),
    box(main, FIT_CTX),
    inputOf(main, FIT_TARGET),
  ])
    expect(el.disabled, 'unchecked: the dependent is disabled (186)').toBe(true)
  fireEvent.click(fitBox(main))
  for (const el of [
    checkBox(main, FIT_CTX),
    box(main, FIT_CTX),
    inputOf(main, FIT_TARGET),
  ])
    expect(el.disabled, 'checked again: enabled (186)').toBe(false)
})

/** 157's `fit-ctx` row (1 or more): the texts it refuses. */
const MIN_CTX_BAD = [{ typed: '0' }, { typed: '-1' }]

test.each(MIN_CTX_BAD)(
  '2h.186, 2h.157: Fit min context refuses "$typed" with the minimum 1, marks it, focuses it, alerts save-invalid, sends nothing, and 1 is sent',
  async (c) => {
    const first = await form()
    type(box(first.main, FIT_CTX), c.typed)
    fireEvent.click(saveButton(first.main))
    await within(first.main).findByText(en('save-invalid'))
    const el = box(first.main, FIT_CTX)
    expectError(el, MIN_ONE, `Fit min context ${c.typed}`)
    expect(document.activeElement, 'focus on Fit min context').toBe(el)
    expectAlert(first.main, en('save-invalid'))
    expect(first.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
    cleanup()
    const second = await form()
    type(box(second.main, FIT_CTX), '1')
    await save(second.main, second.bff)
    expect(sentValues(second.bff)['fit-ctx'], '1 is sent').toBe('1')
  },
)

/** 186: the texts `checks::fit_target` refuses, and the key of the refusal. */
const BAD_TARGET = [
  { typed: '2048;1024', error: MARKERS },
  { typed: 'a', error: TARGET_ERR },
  { typed: '1.5', error: TARGET_ERR },
  { typed: '1024,', error: TARGET_ERR },
  { typed: '1024 2048', error: TARGET_ERR },
  { typed: '-1', error: TARGET_ERR },
  { typed: '1,,2', error: TARGET_ERR },
  { typed: ',', error: TARGET_ERR },
]

test.each(BAD_TARGET)(
  '2h.186: Fit free VRAM "$typed" is refused, marked, focused, save-invalid, nothing sent',
  async (c) => {
    const { bff, main } = await form()
    type(inputOf(main, FIT_TARGET), c.typed)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = inputOf(main, FIT_TARGET)
    expectError(el, c.error, `Fit free VRAM ${c.typed}`)
    expect(document.activeElement, 'focus on Fit free VRAM').toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

/** 186: the texts it accepts, and what 133 sends of them. */
const GOOD_TARGET = [
  { typed: '1024', sent: '1024' },
  { typed: '1024, 2048', sent: '1024, 2048' },
  { typed: ' 1024, 2048 ', sent: '1024, 2048' },
  { typed: '0', sent: '0' },
  { typed: '2048 , 1024', sent: '2048 , 1024' },
  { typed: '2048,1024', sent: '2048,1024' },
  { typed: '', sent: null },
]

test.each(GOOD_TARGET)(
  '2h.186, 2h.159, 2h.133: Fit free VRAM "$typed" passes with no mark, is described by the MiB unit first, and sends $sent',
  async (c) => {
    const { bff, main } = await form()
    const el = inputOf(main, FIT_TARGET)
    expect(
      el.getAttribute('inputmode'),
      '158: a text, no input mode',
    ).toBeNull()
    const uid = unitId(el)
    const unit = document.getElementById(uid)
    expect(unit, `159: the unit has the id ${uid}`).not.toBeNull()
    expect(unit!.textContent, '159: the unit is MiB').toBe(MIB)
    expect(isHidden(unit!), '159: the unit is visible text').toBe(false)
    expect(before(el, unit!), '159: the unit sits after the input').toBe(true)
    expect(describedIds(el)[0], '159: the unit is the first description').toBe(
      uid,
    )
    type(el, c.typed)
    expectNoError(el, TARGET_ERR, `Fit free VRAM "${c.typed}": accepted`)
    expectNoError(el, MARKERS, `Fit free VRAM "${c.typed}": accepted`)
    await save(main, bff)
    expect(
      sentValues(bff)['fit-target'],
      `"${c.typed}": the PUT sends ${JSON.stringify(c.sent)}`,
    ).toBe(c.sent)
  },
)

test('2h.186, 2h.157: a Fit dependent that the unchecked box locks is not checked at Save: a 0 and a list of letters in it send nothing and block nothing', async () => {
  const values = loaded('off', false, false)
  values['fit-ctx'] = '0'
  values['fit-target'] = 'zz'
  const { bff, main } = await form(values)
  expect(box(main, FIT_CTX).disabled, 'Fit min context locked').toBe(true)
  expect(inputOf(main, FIT_TARGET).disabled, 'Fit free VRAM locked').toBe(true)
  expect(box(main, FIT_CTX).value, 'the locked field keeps the file text').toBe(
    '0',
  )
  pick(main, OP, 'false')
  await save(main, bff)
  expect(sentValues(bff), 'only the changed row is sent').toEqual({
    'op-offload': 'false',
  })
  expect(
    within(main).queryByText(en('save-invalid')),
    '157: a locked field is not checked at Save',
  ).toBeNull()
  expectNoError(box(main, FIT_CTX), MIN_ONE, 'the locked 0 is not marked')
  expectNoError(inputOf(main, FIT_TARGET), TARGET_ERR, 'the locked text is not')
})

// ── 184: the text as the user edits the two fields, live ──────────────

test('2h.184: unchecking Fit to VRAM with Context size set gives model-profile-fit-not-allowed, re-checking gives model-profile-fit-layers-only back', async () => {
  const { main } = await form(loaded('on', true, false))
  expect(
    fitNamed(main, LAYERS_ONLY),
    '184: checked with Context size set: layers only',
  ).toBe(fitBox(main))
  fireEvent.click(fitBox(main))
  const off = fitBox(main)
  expect(off.checked, 'the box unchecked').toBe(false)
  expect(
    fitNamed(main, NOT_ALLOWED),
    '184: unchecked with a field set: not-allowed',
  ).toBe(off)
  fireEvent.click(fitBox(main))
  expect(fitNamed(main, LAYERS_ONLY), '184: checked again: layers only').toBe(
    fitBox(main),
  )
})

test('2h.184: unticking GPU layers default box changes the text to model-profile-fit-context-only, ticking it back gives the first text', async () => {
  const { main } = await form(loaded('on', false, false))
  expect(
    checkBox(main, GPU_LAYERS).checked,
    'GPU layers at default: the box ticked',
  ).toBe(true)
  const fit = fitBox(main)
  expect(fitNamed(main, ALLOWED), '184: both at default: allowed').toBe(fit)
  toggle(main, GPU_LAYERS)
  const after = fitBox(main)
  expect(after, 'the same box, not a new one').toBe(fit)
  expect(after.checked, '184: the box keeps its state').toBe(true)
  expect(
    fitNamed(main, CONTEXT_ONLY),
    '184: GPU layers set and checked: context only',
  ).toBe(after)
  toggle(main, GPU_LAYERS)
  expect(fitNamed(main, ALLOWED), '184: back to the first text').toBe(
    fitBox(main),
  )
})

// ── 183, 185, 133: clicks back to the loaded state ────────────────────

/** 183, 133: the loaded spellings, and the written-as line of an alias. */
const FIT_BACK: {
  name: string
  fit: string | undefined
  checked: boolean
  written: string | null
}[] = [
  { name: 'fit on', fit: 'on', checked: true, written: null },
  { name: 'fit off', fit: 'off', checked: false, written: null },
  { name: 'fit absent', fit: undefined, checked: true, written: null },
  {
    name: 'fit true (alias)',
    fit: 'true',
    checked: true,
    written: as_('true'),
  },
]

test.each(FIT_BACK)(
  '2h.183, 2h.185, 2h.133, 2h.134: $name: one click drops the written-as line, two clicks back to the loaded state leave the form clean and a Save sends nothing',
  async (c) => {
    const { bff, main } = await form(loaded(c.fit, false, false))
    expect(fitBox(main).checked, `${c.name}: the loaded state`).toBe(c.checked)
    fireEvent.click(fitBox(main))
    expect(
      fitBox(main).checked,
      `${c.name}: the first click moves the box`,
    ).toBe(!c.checked)
    expect(
      document.getElementById('model-profile-written-fit'),
      '134: a click drops the written-as line (it describes the file)',
    ).toBeNull()
    fireEvent.click(fitBox(main))
    const back = fitBox(main)
    expect(back.checked, `${c.name}: two clicks: the loaded state`).toBe(
      c.checked,
    )
    expectInert(main, `${c.name}: two clicks back`)
    const line = document.getElementById('model-profile-written-fit')
    if (c.written === null)
      expect(
        line,
        `${c.name}: no line for a spelling the configurator writes`,
      ).toBeNull()
    else
      expect(
        line?.textContent,
        '134: the alias line is back with the loaded state',
      ).toBe(c.written)
    fireEvent.click(saveButton(main))
    expect(
      bff.to('PUT', PUT),
      `${c.name}: a clean Save sends nothing`,
    ).toHaveLength(0)
  },
)

test('2h.186: an invalid fit clicked to checked enables the two dependents, unchecked again disables them', async () => {
  const { main } = await form(loaded('TRUE', false, false))
  const locked = () => [
    checkBox(main, FIT_CTX),
    box(main, FIT_CTX),
    inputOf(main, FIT_TARGET),
  ]
  for (const el of locked())
    expect(
      el.disabled,
      '186: an invalid value shows an unchecked box: the dependent is locked',
    ).toBe(true)
  fireEvent.click(fitBox(main))
  expect(fitBox(main).checked, 'the first click checks the box').toBe(true)
  for (const el of locked())
    expect(el.disabled, '186: clicked to checked: the dependent is open').toBe(
      false,
    )
  fireEvent.click(fitBox(main))
  for (const el of locked())
    expect(el.disabled, '186: unchecked again: the dependent is locked').toBe(
      true,
    )
})

// ── 186, 176: a dependent locked while editing ────────────────────────

// 176's "What the page sends, it checks" (the 2h.8 review): a rule-locked
// field whose draft DIFFERS from the loaded value is checked at Save like an
// enabled field, since 176 sends it; the failure marks the field, alerts the
// bar, sends nothing, and focuses the checkbox the rule follows (Fit to VRAM).

test('2h.176, 2h.186: Fit min context locked with a draft that differs from the loaded value is checked at Save: typing 0 over a loaded 4096 and unchecking blocks the save, marks the field, focuses Fit to VRAM, save-invalid, nothing sent', async () => {
  const { bff, main } = await form(loaded('on', false, false))
  type(box(main, FIT_CTX), '0')
  fireEvent.click(fitBox(main))
  const el = box(main, FIT_CTX)
  expect(el.disabled, '186: the unchecked box locks the dependent, live').toBe(
    true,
  )
  expect(el.value, '176: a control a rule disables keeps its draft').toBe('0')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(
    el,
    MIN_ONE,
    '176: a changed locked draft is checked like an enabled field',
  )
  expect(
    document.activeElement,
    '176: focus on the checkbox the rule follows (Fit to VRAM)',
  ).toBe(fitBox(main))
  expectAlert(main, en('save-invalid'))
  expect(bff.to('PUT', PUT), '176: nothing sent').toHaveLength(0)
})

test('2h.176, 2h.186: Fit free VRAM locked with a draft that differs from the loaded value is checked at Save: typing "1024," over a loaded 2048 and unchecking blocks the save, marks the field, focuses Fit to VRAM, save-invalid, nothing sent', async () => {
  const { bff, main } = await form(loaded('on', false, false))
  type(inputOf(main, FIT_TARGET), '1024,')
  fireEvent.click(fitBox(main))
  const el = inputOf(main, FIT_TARGET)
  expect(el.disabled, '186: the unchecked box locks Fit free VRAM, live').toBe(
    true,
  )
  expect(el.value, '176: a control a rule disables keeps its draft').toBe(
    '1024,',
  )
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(
    el,
    TARGET_ERR,
    '176: a changed locked draft is checked like an enabled field',
  )
  expect(
    document.activeElement,
    '176: focus on the checkbox the rule follows (Fit to VRAM)',
  ).toBe(fitBox(main))
  expectAlert(main, en('save-invalid'))
  expect(bff.to('PUT', PUT), '176: nothing sent').toHaveLength(0)
})

test('2h.176: a locked Fit field holding a VALID draft that differs from the loaded value is still sent at Save: type 4096 over a loaded 8192 and uncheck, the PUT carries fit off and fit-ctx 4096', async () => {
  const values = loaded('on', false, false)
  values['fit-ctx'] = '8192'
  const { bff, main } = await form(values)
  type(box(main, FIT_CTX), '4096')
  fireEvent.click(fitBox(main))
  const el = box(main, FIT_CTX)
  expect(el.disabled, '186: the unchecked box locks the dependent').toBe(true)
  expect(el.value, '176: the locked field keeps the valid draft').toBe('4096')
  await save(main, bff)
  const sent = sentValues(bff)
  expect(sent['fit'], 'the unchecked box sends off').toBe('off')
  expect(sent['fit-ctx'], '176: a valid changed locked draft is sent').toBe(
    '4096',
  )
  expect(
    within(main).queryByText(en('save-invalid')),
    '176: a valid draft does not block the save',
  ).toBeNull()
})

// ── 182a: the two layer counts name the words in their refusal ────────

/** The card with the three number fields open: the two layer counts (their
 * keys present, so the default box is unticked), Dense FFN CPU layers (no
 * word), and a `model-draft` so Draft GPU layers is enabled (177). */
function layersFixture(): Record<string, string> {
  return {
    ...loaded('on', false, true),
    'n-cpu-ffn': '4',
    'model-draft': 'Qwen3-27B-DFlash-Q8_0.gguf',
    'n-gpu-layers-draft': '99',
  }
}

/** 182a: the two fields that take `auto` and `all`, and a text that is no
 * whole number and not one of the words. */
const LAYER_WORDS_BAD = [
  { key: 'n-gpu-layers', label: GPU_LAYERS, typed: '1.5' },
  { key: 'n-gpu-layers', label: GPU_LAYERS, typed: 'ALL' },
  { key: 'n-gpu-layers-draft', label: DRAFT_LAYERS, typed: '1.5' },
  { key: 'n-gpu-layers-draft', label: DRAFT_LAYERS, typed: 'ALL' },
]

test.each(LAYER_WORDS_BAD)(
  '2h.182a: $key typed "$typed" is refused with field-error-gpu-layers naming the words, not the general text, marked, focused, save-invalid, nothing sent',
  async (c) => {
    const { bff, main } = await form(layersFixture())
    type(box(main, c.label), c.typed)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = box(main, c.label)
    expectError(el, GPU_ERR, `${c.key} ${c.typed}`)
    expect(
      described(el),
      `${c.key} ${c.typed}: the general text would hide the words`,
    ).not.toContain(WHOLE)
    expect(document.activeElement, `focus on ${c.key}`).toBe(el)
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

/** 182a: the words each of the two fields accepts. */
const LAYER_WORDS_OK = [
  { key: 'n-gpu-layers', label: GPU_LAYERS, typed: 'auto' },
  { key: 'n-gpu-layers', label: GPU_LAYERS, typed: 'all' },
  { key: 'n-gpu-layers-draft', label: DRAFT_LAYERS, typed: 'auto' },
  { key: 'n-gpu-layers-draft', label: DRAFT_LAYERS, typed: 'all' },
]

test.each(LAYER_WORDS_OK)(
  '2h.182a, 2h.157: $key typed "$typed" passes with no mark and sends the word',
  async (c) => {
    const { bff, main } = await form(layersFixture())
    const el = box(main, c.label)
    type(el, c.typed)
    await save(main, bff)
    expectNoError(el, GPU_ERR, `${c.key} ${c.typed}`)
    expect(sentValues(bff)[c.key], `${c.key} ${c.typed} is sent`).toBe(c.typed)
  },
)

test('2h.182a: Dense FFN CPU layers typed 1.5 keeps the general whole-number error, since the key takes no word', async () => {
  const { bff, main } = await form(layersFixture())
  type(box(main, FFN), '1.5')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const el = box(main, FFN)
  expectError(el, WHOLE, FFN)
  expect(
    described(el),
    `${FFN}: the words text is for the layer counts only`,
  ).not.toContain(GPU_ERR)
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

// ── 188: the four new strings ─────────────────────────────────────────

test('2h.188: the new strings model-profile-fit-layers-only, model-profile-fit-context-only, model-profile-fit-nothing and field-error-fit-target are the briefs sentences, word for word', () => {
  expect(en('model-profile-fit-layers-only')).toBe(
    'auto-fit the GPU layers only: Context size is set (-fit on)',
  )
  expect(en('model-profile-fit-context-only')).toBe(
    'auto-fit the context only: GPU layers is set, so llama.cpp keeps your layers (-fit on)',
  )
  expect(en('model-profile-fit-nothing')).toBe(
    'nothing to fit: Context size and GPU layers are both set (-fit on)',
  )
  expect(en('field-error-fit-target')).toBe(
    'Enter MiB as whole numbers: one number, or one per device separated by commas, such as 2048,1024.',
  )
})

// ── 182a: the new string ──────────────────────────────────────────────

test('2h.182a: field-error-gpu-layers is in the fe strings file, the briefs sentence word for word, the words lower case', () => {
  expect(en('field-error-gpu-layers'), 'the test table').toBe(
    'Enter a whole number, auto or all.',
  )
  expect(t('field-error-gpu-layers'), 'the fe strings file').toBe(
    'Enter a whole number, auto or all.',
  )
})
