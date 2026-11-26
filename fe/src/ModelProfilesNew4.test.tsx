// Brief 22, slice 22C2 (fe/briefs/22-model-sources.md, as the "22P/22C check
// (2026-11-26)" and the 2026-11-25 "22P check" amend it): the New model profile
// form's Hugging Face half, the reading of it. This file is 22 P.1 (the slice
// cut: the third choice, where it sits and what it keeps), 22 P.7 (the repo
// field, its hint, the Show quants button, the status line and the alert
// place), 22 P.8 (the text reduced to a repo id, the read, its loading line and
// its inactive button) and 22 P.9 with P.9a (the quants fieldset, its radios,
// the pick on arrival, the create of the pick, and a 200 with no files) with
// 22 C.5 (the pick a create sends). 22
// P.10 with P.10a (the refusals), 22 P.11 (an edit drops what was listed) and
// the Hugging Face halves of 22 C.4 and C.6 are ModelProfilesNew5; the form's
// shape, the file half and the checks of the other choices are
// ModelProfilesNew1, New2 and New3. 22 P.12 (the strings file) is `1a strings`
// in App.test.tsx, and its widths are the UX check's `model-profiles-new-hf`
// state.
//
// Enter in a text field submits a form in a browser; jsdom does not do it, so
// 22 P.7's "Enter in the input does not press it" is pinned as the two halves
// observable here: the keydown is not cancelled (so the browser submits) and
// the listing is not read.
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
  HF_FILES,
  HF_REPO,
  before,
  deferred,
  en,
  fakeBff,
  hfFilesBody,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type HfFileRow,
  type ModelProfile,
} from './test/bff.ts'

const CREATE = '/api/profiles/model'

const FILES = '/api/hf/files'

const QWEN = '/var/lib/llama.cpp/models/qwen/Qwen3-27B-Q8_0.gguf'

let dialogs: ReturnType<typeof stubDialog>

function modelProfile(id: string, enabled: boolean, model = ''): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

/** The list behind the form: two profiles, so the copy radio is there. */
const LIST_ROWS: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, QWEN),
  modelProfile('Old-Coder', false),
]

/** The fixture's default quant, the pick a listing opens on. */
const DEFAULT_ROW = HF_FILES.find((f) => f.default)!

/** The fixture's row whose tag is shared, so its value names a hf-file. */
const SHARED_ROW = HF_FILES.find((f) => f.value['hf-file'] !== undefined)!

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

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The app at #/profiles/model, the table loaded. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST_ROWS, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function key(el: HTMLElement, k: string) {
  return fireEvent.keyDown(el, { key: k, code: k })
}

/** The create form, opened by the +. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(within(main).getByRole('button', { name: en('model-profiles-new') }))
  return within(main).findByRole('form', {
    name: en('model-profiles-new-title'),
  })
}

/** The create form, on the Hugging Face choice. */
async function openHf(main: HTMLElement): Promise<HTMLElement> {
  const form = await openForm(main)
  press(radioIn(form, 'model-source-hf'))
  await hfBox(form)
  return form
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, keyName: string): HTMLElement {
  return within(form).getByRole('button', { name: en(keyName) })
}

function radioIn(form: HTMLElement, keyName: string): HTMLInputElement {
  return within(form).getByRole('radio', {
    name: en(keyName),
  }) as HTMLInputElement
}

/** The repo input. */
function hfBox(form: HTMLElement): HTMLInputElement {
  return within(form).getByRole('textbox', {
    name: en('model-profile-hf-repo'),
  }) as HTMLInputElement
}

/** Show quants. */
function showButton(form: HTMLElement): HTMLElement {
  return buttonIn(form, 'hf-show-quants')
}

/** One text, whitespace collapsed. */
function text(el: Node | null | undefined): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The quants' fieldset, the one of the form whose legend is hf-quant-legend.
 *  A fieldset is a `group`, not a `radiogroup`, so it is read by its legend. */
function quantGroups(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll('fieldset')].filter(
    (f) => text(f.querySelector('legend')) === en('hf-quant-legend'),
  )
}

function quantGroup(form: HTMLElement): HTMLElement {
  const open = quantGroups(form).filter((f) => !isHidden(f))
  expect(open, 'one open fieldset legend hf-quant-legend').toHaveLength(1)
  return open[0]!
}

/** The quants' radios, in the document's order. */
function quantRadios(form: HTMLElement): HTMLInputElement[] {
  const group = quantGroup(form)
  return within(form)
    .getAllByRole('radio')
    .filter((r) => r.closest('fieldset') === group) as HTMLInputElement[]
}

/** What the eye reads beside a radio: its `label`, by `for` or by wrapping. */
function radioLabel(r: HTMLInputElement): string {
  const named = r.id
    ? document.querySelector(`label[for="${r.id}"]`)
    : undefined
  return text(named ?? r.closest('label') ?? undefined)
}

/** What one quant's label reads (22 P.9): the tag, `, ` the size, `, ` the
 *  parts when it is split, `, ` the default mark when it is the default. */
function quantLabel(row: HfFileRow): string {
  const parts =
    row.shards > 1 ? `, ${en('model-file-parts', `${row.shards}`)}` : ''
  const mark = row.default ? `, ${en('hf-quant-default')}` : ''
  return `${row.tag}, ${size(row.size)}${parts}${mark}`
}

/** bytes as the code writes them (brief 22 section 2). */
function size(bytes: number): string {
  const gib = bytes / 2 ** 30
  return gib < 0.1
    ? `${Math.round(bytes / 2 ** 20)} MiB`
    : `${gib.toFixed(1)} GiB`
}

/** The text of every element named by `el`'s `aria-describedby`. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Every tab stop inside `root`, in the document's order. */
function tabbables(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('[hidden]') === null,
  )
}

/** The listing reads the bff was asked for, in order. */
function hfReads(bff: ReturnType<typeof fakeBff>) {
  return bff.calls.filter((c) => c.path.startsWith(FILES))
}

/** The repo each listing read asked for, in order. */
function hfRepos(bff: ReturnType<typeof fakeBff>) {
  return hfReads(bff).map(
    (c) => new URL(c.path, 'http://x').searchParams.get('repo') ?? '',
  )
}

/** The creates the bff was asked for, in order. */
function posts(bff: ReturnType<typeof fakeBff>) {
  return bff.to('POST', CREATE)
}

/** The status lines inside `root`, in the document's order. */
function statusLines(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="status"]')]
}

/** The text of every status line inside `root`, whitespace collapsed. */
function statusTexts(root: HTMLElement): string[] {
  return statusLines(root).map(text)
}

/** The text of every alert inside `root`, whitespace collapsed. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll<HTMLElement>('[role="alert"]')].map(text)
}

/** A form on the Hugging Face choice, `value` in the repo field, Show quants
 *  pressed, its read answered by `answer`. */
async function shown(
  value: string,
  answer: () => Response | Promise<Response>,
  options: BffOptions = {},
) {
  const { bff, main } = await loaded({ hfFiles: answer, ...options })
  const form = await openHf(main)
  const box = hfBox(form)
  type(box, value)
  const button = showButton(form)
  press(button)
  return { bff, main, form, box, button }
}

// ── 22 P.1 (the slice cut): the third choice ────────────────────────────

test('22 P.1, the slice cut: the picker is the file, the Hugging Face and the copy radios in that order; the Hugging Face field shows only when its choice is checked, and switching keeps what each choice holds and moves no focus', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const file = radioIn(form, 'model-source-file')
  const hf = radioIn(form, 'model-source-hf')
  const copy = radioIn(form, 'model-source-copy')
  expect(
    within(form)
      .getAllByRole('radio')
      .filter((r) => !isHidden(r)),
    'three choices, in the order the slice cut gives',
  ).toEqual([file, hf, copy])
  expect(hf.checked, '22 C.1: the file is checked, not Hugging Face').toBe(
    false,
  )
  // The Hugging Face field shows only with its choice checked. Closed it sits
  // under a `[hidden]` ancestor, so it answers no role query: the closure is
  // pinned as a `queryByRole` null, as ModelProfilesNew1 does for the copy
  // select, and the box is taken once its choice shows it.
  expect(
    within(form).queryByRole('textbox', {
      name: en('model-profile-hf-repo'),
    }),
    'the repo field is closed at first',
  ).toBeNull()
  const fileBox = within(form).getByRole('combobox', {
    name: en('model-profile-model-file'),
  }) as HTMLInputElement
  expect(isHidden(fileBox), 'the file field shows at first').toBe(false)
  press(hf)
  const box = hfBox(form)
  expect(isHidden(box), 'the repo field shows with its choice').toBe(false)
  expect(isHidden(fileBox), 'the file field is gone from the flow').toBe(true)
  // Each choice keeps what it holds (22 P.1).
  type(box, 'ggml-org/gemma')
  type(fileBox, 'qwen')
  press(file)
  expect(fileBox.value, 'the file text is kept').toBe('qwen')
  expect(isHidden(box), 'the repo field is closed again').toBe(true)
  press(hf)
  expect(box.value, 'the repo text is kept').toBe('ggml-org/gemma')
  expect(isHidden(fileBox), 'the file field is closed').toBe(true)
  expect(document.activeElement, 'the focus did not move').toBe(hf)
})

// ── 22 P.7: the repo field, the button, the status line ─────────────────

test('22 P.7: the repo field is labelled model-profile-hf-repo with its hint behind the About toggle, its input is a text input with autocomplete off and spellcheck false, a secondary Show quants follows it, and the form has its status line and an empty alert place', async () => {
  const { main } = await loaded()
  const form = await openHf(main)
  const box = hfBox(form)
  const field = box.closest('.field')
  expect(field, 'the input is in its field').not.toBeNull()
  const hint = within(field as HTMLElement).getByText(
    en('model-profile-hf-repo-hint'),
  )
  expect(isHidden(hint), 'the hint starts closed').toBe(true)
  press(
    within(field as HTMLElement).getByRole('button', {
      name: `${en('field-about')} ${en('model-profile-hf-repo')}`,
    }),
  )
  expect(isHidden(hint), 'About shows the hint').toBe(false)
  expect(box.tagName, 'a native input').toBe('INPUT')
  expect(box.getAttribute('type') ?? 'text', 'a text input').toBe('text')
  expect(box.getAttribute('autocomplete'), 'no browser fill').toBe('off')
  expect(box.getAttribute('spellcheck'), 'no spellcheck').toBe('false')
  expect(box.getAttribute('aria-invalid'), 'nothing invalid at rest').toBeNull()
  // The button: secondary, after the field (fe/DESIGN.md 5.2).
  const button = showButton(form)
  expect(button.tagName, 'a native button').toBe('BUTTON')
  expect(
    button.classList.contains('secondary'),
    'Show quants is a secondary button',
  ).toBe(true)
  expect(before(box, button), 'the button is after the input').toBe(true)
  // The status line, and the alert place, empty at rest.
  expect(
    statusLines(form).length,
    'the form has a status line',
  ).toBeGreaterThan(0)
  expect(alertTexts(form), 'no alert at rest').toEqual([])
  expect(within(form).queryAllByRole('alert'), 'no alert at rest').toHaveLength(
    0,
  )
})

test('22 P.7: Show quants is a separate tab stop right after the input, and Enter in the input does not press it', async () => {
  const { bff, main } = await loaded()
  const form = await openHf(main)
  const box = hfBox(form)
  const button = showButton(form)
  const stops = tabbables(main)
  const at = (el: HTMLElement) => stops.indexOf(el)
  expect(at(box), 'the input is a tab stop').toBeGreaterThan(-1)
  expect(at(button), 'the button is a tab stop').toBeGreaterThan(-1)
  expect(at(button), 'the button is the stop right after the input').toBe(
    at(box) + 1,
  )
  expect(key(box, 'Enter'), 'Enter is not cancelled, so the form submits').toBe(
    true,
  )
  expect(hfReads(bff), 'Enter did not press Show quants').toHaveLength(0)
  expect(posts(bff), 'and did not send a create').toHaveLength(0)
})

// ── 22 P.8: Show quants ────────────────────────────────────────────────

test('22 P.8, P.10a: a valid repo sends GET /api/hf/files?repo=<owner/name> once; meanwhile the status reads hf-quants-loading with the repo and the button is aria-disabled, and a second press sends nothing', async () => {
  const gate = deferred()
  const { bff, form, button } = await shown(HF_REPO, () => gate.promise)
  expect(hfRepos(bff), 'the repo is asked for as owner/name').toEqual([HF_REPO])
  const line = statusLines(form).find((p) =>
    text(p).includes(en('hf-quants-loading', HF_REPO)),
  )
  expect(line, 'the status line names the repo being read').not.toBeUndefined()
  expect(
    line!.closest('[role="status"]'),
    'the loading text is a status line',
  ).not.toBeNull()
  expect(button.getAttribute('aria-disabled'), 'the button is inactive').toBe(
    'true',
  )
  press(button)
  expect(hfReads(bff), 'a second press sends nothing').toHaveLength(1)
  gate.resolve(reply(200, hfFilesBody()))
  await flushTasks()
  expect(
    button.getAttribute('aria-disabled'),
    'the button is live again',
  ).toBeNull()
})

test('22 P.8: a huggingface.co, www.huggingface.co or hf.co address is reduced to its first two path segments, and a :TAG end is set apart from the repo asked for', async () => {
  const cases: [string, string][] = [
    ['https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF', HF_REPO],
    ['https://www.huggingface.co/ggml-org/gemma-3-1b-it-GGUF', HF_REPO],
    ['http://hf.co/ggml-org/gemma-3-1b-it-GGUF', HF_REPO],
    ['huggingface.co/ggml-org/gemma-3-1b-it-GGUF', HF_REPO],
    [
      'https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/tree/main/Q4_K_M.gguf',
      HF_REPO,
    ],
    [`${HF_REPO}:Q4_K_M`, HF_REPO],
    [`${HF_REPO}:q4_k_m`, HF_REPO],
    [`https://huggingface.co/${HF_REPO}:Q4_K_M`, HF_REPO],
  ]
  for (const [value, repo] of cases) {
    const { bff } = await shown(value, () => reply(200, hfFilesBody()))
    expect(hfRepos(bff), `${value}: one read`).toEqual([repo])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('22 P.8: text that is not a repo id shows field-error-hf-repo under the field, marks it aria-invalid and described by it, moves the focus there and sends nothing', async () => {
  const cases = [
    'gemma',
    'ggml-org/gemma/too',
    'ggml org/gemma',
    'ggml-org/gemm..',
    'ggml-/gemma',
    '/gemma',
    'owner/a'.concat('b'.repeat(256)),
    'htp://huggingface.co/ggml-org/gemma',
  ]
  for (const value of cases) {
    const { bff, form, box } = await shown(value, () =>
      reply(200, hfFilesBody()),
    )
    const error = await within(form).findByText(en('field-error-hf-repo'))
    expect(before(box, error), `${value}: the error is under the field`).toBe(
      true,
    )
    expect(
      box.getAttribute('aria-invalid'),
      `${value}: the field is marked`,
    ).toBe('true')
    expect(described(box), `${value}: described by its error`).toContain(
      en('field-error-hf-repo'),
    )
    expect(document.activeElement, `${value}: the focus is on the field`).toBe(
      box,
    )
    expect(
      within(form).queryAllByRole('alert'),
      `${value}: nothing in the alert`,
    ).toHaveLength(0)
    expect(hfReads(bff), `${value}: nothing sent`).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 22 P.9: the quants ─────────────────────────────────────────────────

test('22 P.9: the 200 shows, after the button, a fieldset legend hf-quant-legend with one radio per file in the reply order, each named by its tag, its size, its parts and its default mark; the default one is checked on arrival, sets the field text to its value, keeps the focus on the button and leaves the status line saying nothing more', async () => {
  const { bff, form, box, button } = await shown(HF_REPO, () =>
    reply(200, hfFilesBody()),
  )
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  const group = quantGroup(form)
  expect(group.tagName, 'a fieldset').toBe('FIELDSET')
  expect(text(group.querySelector('legend')), 'the legend').toBe(
    en('hf-quant-legend'),
  )
  expect(
    before(showButton(form), group),
    'the fieldset is after the button',
  ).toBe(true)
  const radios = quantRadios(form)
  expect(
    radios.map(radioLabel),
    'each radio is named by its tag, its size, its parts and its default mark',
  ).toEqual(HF_FILES.map(quantLabel))
  const checked = radios.filter((r) => r.checked)
  expect(checked, 'one quant is picked on arrival').toHaveLength(1)
  expect(radios.indexOf(checked[0]!), '22 P.9: the default one').toBe(
    HF_FILES.indexOf(DEFAULT_ROW),
  )
  expect(box.value, 'the field carries the pick').toBe(
    DEFAULT_ROW.value['hf-repo'],
  )
  expect(document.activeElement, 'the focus stayed on the button').toBe(button)
  expect(statusTexts(form), 'the status line says nothing more').not.toContain(
    en('hf-quants-loading', HF_REPO),
  )
  expect(hfReads(bff), 'read once').toHaveLength(1)
})

test('22 P.9: the file whose tag equals the field text is picked on arrival, letter case aside; with no tag and no default none is picked and the field keeps the typed text', async () => {
  const noDefault = HF_FILES.map((f) => ({ ...f, default: false }))
  for (const [value, tag] of [
    [`${HF_REPO}:Q8_0`, 'Q8_0'],
    [`${HF_REPO}:q8_0`, 'Q8_0'],
    [`https://huggingface.co/${HF_REPO}:F16`, 'F16'],
  ] as const) {
    const { form, box } = await shown(value, () =>
      reply(200, hfFilesBody(noDefault)),
    )
    await waitFor(() => expect(quantRadios(form).length).toBe(noDefault.length))
    const want = noDefault.find((f) => f.tag === tag)!
    const radios = quantRadios(form)
    const checked = radios.filter((r) => r.checked)
    expect(checked, `${value}: one pick`).toHaveLength(1)
    expect(
      radios.indexOf(checked[0]!),
      `${value}: the tag picks its file`,
    ).toBe(noDefault.indexOf(want))
    expect(box.value, `${value}: the field carries the pick`).toBe(
      want.value['hf-repo'],
    )
    cleanup()
    vi.unstubAllGlobals()
  }
  // No tag, no default: no pick, and the text stays as typed.
  const { form, box } = await shown(HF_REPO, () =>
    reply(200, hfFilesBody(noDefault)),
  )
  await waitFor(() => expect(quantRadios(form).length).toBe(noDefault.length))
  expect(
    quantRadios(form).filter((r) => r.checked),
    '22 P.9: with no default, none is picked',
  ).toHaveLength(0)
  expect(box.value, 'the field keeps what was typed').toBe(HF_REPO)
})

test('22 P.9, C.5: checking a quant sets the field text to its value["hf-repo"] and keeps its value["hf-file"], which the create sends as hf-file', async () => {
  const gate = deferred()
  // The gate holds the create, not the listing: the quants must list for the
  // pick to be checked, and the create must stay open while its body is read.
  const { bff, form, box } = await shown(
    HF_REPO,
    () => reply(200, hfFilesBody()),
    {
      modelProfileCreate: () => gate.promise,
    },
  )
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  press(quantRadios(form)[HF_FILES.indexOf(SHARED_ROW)]!)
  expect(box.value, 'the field carries the pick').toBe(
    SHARED_ROW.value['hf-repo'],
  )
  type(boxIn(form, en('model-profile-name')), 'New-Model')
  press(buttonIn(form, 'create-button'))
  expect(posts(bff)[0]!.body, 'the pick is the pair of the contract').toEqual({
    id: 'New-Model',
    values: {
      'hf-repo': SHARED_ROW.value['hf-repo'],
      'hf-file': SHARED_ROW.value['hf-file'],
    },
  })
  gate.resolve(reply(201, { id: 'New-Model' }))
  await flushTasks()
})

test('22 C.5: the Hugging Face pick sends POST /api/profiles/model with the trimmed id and values hf-repo of the pick, and the 201 goes to the new profile page with its h1 focused', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileCreate: () => gate.promise })
  const form = await openHf(main)
  type(hfBox(form), HF_REPO)
  press(showButton(form))
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  press(quantRadios(form)[HF_FILES.indexOf(DEFAULT_ROW)]!)
  type(boxIn(form, en('model-profile-name')), '  New-Model  ')
  press(buttonIn(form, 'create-button'))
  expect(posts(bff)[0]!.body, 'the body of the contract').toEqual({
    id: 'New-Model',
    values: { 'hf-repo': DEFAULT_ROW.value['hf-repo'] },
  })
  gate.resolve(reply(201, { id: 'New-Model' }))
  await flushTasks()
  expect(window.location.hash, 'the new profile page').toBe(
    '#/profiles/model/New-Model',
  )
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'New-Model',
  })
  expect(document.activeElement, '22 C.5a: its h1 takes the focus').toBe(h1)
})

test('22 P.9a: a 200 with no files is not an empty listing: it reads as api-hf-bad-answer in the alert, with no fieldset, and the field keeps its text', async () => {
  const { bff, form, box } = await shown(HF_REPO, () =>
    reply(200, hfFilesBody([])),
  )
  const alert = await within(form).findByText(en('api-hf-bad-answer'))
  expect(
    alert.closest('[role="alert"]'),
    'the empty listing is in the alert place',
  ).not.toBeNull()
  expect(quantGroups(form), 'no fieldset').toHaveLength(0)
  expect(box.value, 'the field keeps its text').toBe(HF_REPO)
  expect(hfReads(bff), 'read once').toHaveLength(1)
})
