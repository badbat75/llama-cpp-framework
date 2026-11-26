// Brief 21, sub-slice TP3 (fe/briefs/21-tensor-placement.md), criteria 21 T.14,
// 21 T.14a, 21 T.15, 21 T.15a, 21 T.16 and 21 T.17, as the "TP3 check
// (2026-11-26)" section amends them: the coverage line under each rule, the
// tensor list of the page's own model-info read, the cut list, and TP3's
// strings. The counting itself, copied from the GUI's two Rust vectors, is
// `tensorRules.test.ts`; the parse of the read is `apiModelInfo.test.ts`. The
// helpers are those of `TensorPlacement1/2/3.test.tsx`, repeated here (a test
// file imports no other test file), and so is the stylesheet reader: the line's
// style is pinned on the sheet, since jsdom lays nothing out. The 320 px and
// 200 % half of 21 T.17 is the designer's measurement of the ux state
// `model-profile-placement` (21 T.17a), whose stub list is checked as text.
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
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  DEVICES,
  en,
  fakeBff,
  held,
  MODEL_FILE,
  modelProfileDoc,
  quiet,
  reply,
  startAt,
  type BffOptions,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const INFO = `/api/profiles/model/${ID}/model-info`
const CARD = en('model-profile-tensor-placement')
const RULES = en('model-profile-ot-rules')
const TENSOR = en('model-profile-ot-tensor')
const LAYERS = en('model-profile-ot-layers')
const GOES = en('model-profile-ot-goes-on')
const PATTERN = en('model-profile-ot-pattern')
const TEXT = en('model-profile-ot-text')
const ADD = en('model-profile-ot-add')
/** The paragraph the Rules fieldset describes itself with (21 T.2): the cut
 *  line goes right after it. */
const FORMAT = 'model-profile-ot-format'
/** The key this card owns. */
const OT = 'override-tensor'
/** The three rules of the ux state (brief 21, section 7): a MoE experts rule
 *  with layers, the embedding table, a hand-written pattern. */
const R1 = String.raw`blk\.(20|21|22|23)\.ffn_(up|down|gate|gate_up)_(ch|)exps`
const R2 = String.raw`^token_embd\.weight`
const R3 = String.raw`blk\.1[0-9]\..*_exps`
/** A hand-written pattern no kind owns: its row has a Pattern input. */
const HAND = R3

const AS_LOADED: Record<string, string> = {
  model: MODEL_FILE,
  'n-gpu-layers': '29',
}

/** `AS_LOADED` with these rules, every one on CPU. */
function file(...rules: string[]): Record<string, string> {
  return { ...AS_LOADED, [OT]: rules.map((r) => `${r}=CPU`).join(',') }
}

// ── The model's tensors (21 T.14) ─────────────────────────────────────────

/** The stub's list (21 T.17a): 64 blocks, each with its three expert tensors
 *  and its attention, plus the embedding table and the output head. An expert
 *  tensor is 1 MiB, so "N tensors, N.0 MiB" reads straight; the embedding is
 *  1.21 GiB and the head 512 MiB, so the one-tensor text shows too. */
const BLOCKS = 64
const EXPERT = 1 << 20
const EMBD_BYTES = 1299227607
const OUTPUT_BYTES = 1 << 29

function tensorList(): { name: string; bytes: number }[] {
  const v: { name: string; bytes: number }[] = [
    { name: 'token_embd.weight', bytes: EMBD_BYTES },
  ]
  for (let b = 0; b < BLOCKS; b++)
    for (const p of ['up', 'down', 'gate'])
      v.push({ name: `blk.${b}.ffn_${p}_exps.weight`, bytes: EXPERT })
  for (let b = 0; b < BLOCKS; b++)
    v.push({ name: `blk.${b}.attn_q.weight`, bytes: EXPERT })
  v.push({ name: 'output.weight', bytes: OUTPUT_BYTES })
  return v
}

/** The facts of the page's read, with the tensor list, and `extra` on top of
 *  it (`tensors` or `tensorsTruncated` set to something else). */
function facts(extra: Record<string, unknown> = {}): ModelInfo {
  return {
    model: MODEL_FILE,
    info: {
      blockCount: BLOCKS,
      moe: true,
      expertCount: 128,
      expertUsed: 8,
      tensors: tensorList(),
      tensorsTruncated: false,
      ...extra,
    },
    reason: null,
  }
}

/** The line of `n` tensors of 1 MiB each (21 T.15). */
const many = (n: number) =>
  en('model-profile-ot-tensors-many', `${n}`, `${n}.0 MiB`)
/** The line of one tensor of `bytes` (21 T.15). */
const one = (bytes: string) => en('model-profile-ot-tensors-one', bytes)

beforeEach(() => {
  startAt(HASH)
  // jsdom has no scrollIntoView; the card's jump button calls it.
  Element.prototype.scrollIntoView = () => {}
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Waits for the page read with the file `values`, the facts `facts()` (the
 *  tensor list included) and the probe `DEVICES`, unless `options` says
 *  otherwise. A save answers 200 and applies the sent values. */
async function form(
  values: Record<string, string> = { ...AS_LOADED },
  options: BffOptions = {},
) {
  const loaded = { ...values }
  const bff = fakeBff({
    devices: DEVICES,
    modelInfo: facts(),
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { values: { ...loaded } })),
    modelProfileSave: (body) => {
      const sent = (body as { values?: Record<string, unknown> })?.values ?? {}
      for (const [key, value] of Object.entries(sent)) {
        if (value === null || value === undefined) delete loaded[key]
        else loaded[key] = String(value)
      }
      return reply(200, { revision: 'r2', running: false })
    },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: CARD })
  return { bff, main }
}

/** The device read's as-of line shows: the page's reads are over, so the
 *  placement card is drawn with the devices it offers. */
async function landed(main: HTMLElement) {
  const fs = await within(main).findByRole('group', {
    name: en('model-profile-gpus'),
  })
  await within(fs).findByText(
    new Date(String(DEVICES.probedAt)).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }),
  )
}

function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The card's Rules fieldset, and the wrapper `div` inside it (21 T.2). */
function rulesFieldset(card: HTMLElement): HTMLElement {
  return within(card).getByRole('group', { name: RULES })
}

function wrapperOf(fieldset: HTMLElement): HTMLElement {
  const div = fieldset.querySelector(':scope > div')
  expect(div, 'the Rules fieldset has its wrapper div').not.toBeNull()
  return div as HTMLElement
}

function ruleGroup(card: HTMLElement, n: number): HTMLElement {
  return within(card).getByRole('group', {
    name: en('model-profile-ot-rule', `${n}`),
  })
}

/** The rule's wrapping row (TP1b, `.ot-rule-row`). */
function rowOf(group: HTMLElement): HTMLElement {
  const row = group.querySelector('[class*="ot-rule-row"]')
  expect(row, 'the rule has its row').not.toBeNull()
  return row as HTMLElement
}

function tensorOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: TENSOR,
  }) as HTMLSelectElement
}

function layersOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: LAYERS,
  }) as HTMLInputElement | null
}

function goesOnOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: GOES,
  }) as HTMLSelectElement
}

function patternOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: PATTERN,
  }) as HTMLInputElement | null
}

function textField(card: HTMLElement): HTMLInputElement {
  return within(card).getByRole('textbox', { name: TEXT }) as HTMLInputElement
}

/** Types into a control, as a keystroke does. */
function type(el: HTMLElement, value: string) {
  fireEvent.change(el, { target: { value } })
}

/** The ids named by `aria-describedby`, in order. */
function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
}

/** The last id `aria-describedby` names (21 T.15: the line, last). */
function describedLast(el: HTMLElement): string | undefined {
  const ids = describedIds(el)
  return ids[ids.length - 1]
}

/** The Field Error element the control names, by id. */
function errorOf(el: HTMLElement, text: string): HTMLElement | null {
  return (
    describedIds(el)
      .map((id) => document.getElementById(id))
      .find((e) => e !== null && e.textContent === text) ?? null
  )
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** Clicks Save and waits for the bar's `save-invalid`: the page's own check
 *  refused, so nothing went out (21 T.11). */
async function saveInvalid(main: HTMLElement) {
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
}

/** The rule row's coverage line (21 T.15a): the plain paragraph that is the
 *  row's LAST child, after its Field Errors. `null` when the row has none: its
 *  last child is a control item, or a Field Error. */
function coverageOf(group: HTMLElement): HTMLElement | null {
  const last = rowOf(group).lastElementChild
  if (last === null || last.tagName !== 'P') return null
  if (last.classList.contains('field-error')) return null
  return last as HTMLElement
}

/** Every rule's line, in rule order: `null` where a rule has none. */
function coverageLines(card: HTMLElement, n: number): (HTMLElement | null)[] {
  return Array.from({ length: n }, (_, i) => coverageOf(ruleGroup(card, i + 1)))
}

/** The same, as text: `null` where a rule has no line. */
function coverageTexts(card: HTMLElement, n: number): (string | null)[] {
  return coverageLines(card, n).map((line) => line?.textContent?.trim() ?? null)
}

// ── The stylesheet reader (the way `Reflow.test.ts` reads `index.css`) ─────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

/** Every rule of the sheet, the ones inside an `@media` block included: the
 *  prelude holds a brace, so the regex reads the inner rules and drops it. */
const SHEET_RULES: { sel: string; body: string }[] = [
  ...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g),
].map((m) => ({ sel: m[1].trim(), body: m[2] }))

/** The declarations the sheet applies to `el`: every rule with a selector that
 *  matches it, in the sheet's order. The selectors are matched against the
 *  element on the page, so the pin follows the names the coder chose. */
function sheetDeclarations(el: Element): Record<string, string> {
  const out: Record<string, string> = {}
  for (const { sel, body } of SHEET_RULES) {
    let hit = false
    for (const part of sel.split(',')) {
      try {
        if (el.matches(part.trim())) {
          hit = true
          break
        }
      } catch {
        // A selector jsdom's `matches` refuses (`:scope`, a pseudo-element).
      }
    }
    if (!hit) continue
    for (const d of body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  }
  return out
}

const norm = (v: string | undefined): string => (v ?? '').replace(/\s+/g, ' ')

// ── 21 T.15a: the line under each rule ────────────────────────────────────

test('21 T.15a: with a tensor list, every rule row ends with its coverage line: the last child of .ot-rule-row, a plain paragraph, counting the rule as written into override-tensor', async () => {
  const { main } = await form(file(R1, R2, R3))
  await landed(main)
  const card = cardOf(main, CARD)
  const want = [many(12), one('1.21 GiB'), many(30)]
  for (const [i, text] of want.entries()) {
    const group = ruleGroup(card, i + 1)
    const row = rowOf(group)
    const line = coverageOf(group)
    expect(line, `rule ${i + 1}: the row ends with its line`).not.toBeNull()
    expect(
      row.lastElementChild,
      `rule ${i + 1}: the line is the row's last child`,
    ).toBe(line)
    expect(line!.tagName, `rule ${i + 1}: a plain paragraph`).toBe('P')
    expect(line!.id, `rule ${i + 1}: the line has an id`).not.toBe('')
    expect(line!.textContent!.trim(), `rule ${i + 1}: the text`).toBe(text)
  }
})

test('21 T.15a: the line is styled as the Field Errors are: flex-basis 100%, --muted, --fs-sm, overflow-wrap anywhere', async () => {
  const { main } = await form(file(R1, R2, R3))
  await landed(main)
  const line = coverageOf(ruleGroup(cardOf(main, CARD), 1))
  expect(line, 'the line is there').not.toBeNull()
  const decl = sheetDeclarations(line!)
  expect(norm(decl['flex-basis']), 'a line of its own').toBe('100%')
  expect(norm(decl['color']), 'muted text').toBe('var(--muted)')
  expect(norm(decl['font-size']), 'the small text token').toBe('var(--fs-sm)')
  expect(norm(decl['overflow-wrap']), 'wraps anywhere').toBe('anywhere')
})

test('21 T.15a: the line sits after the rule Field Errors, and a marked rule keeps its line', async () => {
  // The rule has no device (the piece has no `=`), so the page's own check
  // marks its Goes on select at Save; the line is not a check and stays.
  const { main } = await form({ ...AS_LOADED, [OT]: HAND })
  await landed(main)
  const card = cardOf(main, CARD)
  const group = ruleGroup(card, 1)
  const goes = goesOnOf(group)
  expect(goes.value, '21 T.11: the rule is on no device').toBe('')
  await saveInvalid(main)
  const error = errorOf(goes, en('model-profile-ot-device-missing'))
  expect(error, '21 T.11: the rule is marked').not.toBeNull()
  const line = coverageOf(group)
  expect(line, '21 T.15a: the line is not taken by the mark').not.toBeNull()
  expect(before(error!, line!), 'the line comes after the Field Error').toBe(
    true,
  )
  expect(
    rowOf(group).lastElementChild,
    'the line is the row last child, after the Field Error',
  ).toBe(line)
  expect(line!.textContent!.trim(), 'the rule counts its tensors').toBe(
    many(30),
  )
})

test('21 T.15a: the pattern counted is the rule as written into override-tensor: a MoE experts rule with layers 20-23 counts the 12 tensors its composed pattern matches', async () => {
  const { main } = await form(file())
  await landed(main)
  const card = cardOf(main, CARD)
  fireEvent.click(within(card).getByRole('button', { name: ADD }))
  const group = ruleGroup(card, 1)
  fireEvent.change(tensorOf(group), { target: { value: 'exps' } })
  const layers = layersOf(group)
  expect(layers, 'the kind has a Layers field').not.toBeNull()
  type(layers!, '20-23')
  await waitFor(() =>
    expect(
      coverageOf(ruleGroup(card, 1))?.textContent?.trim(),
      'the composed pattern blk\\.(20|21|22|23)\\.ffn_(up|down|gate|gate_up)_(ch|)exps',
    ).toBe(many(12)),
  )
})

test('21 T.10, 21 T.15a: an edit in Rules as text redraws the rows and their lines', async () => {
  const { main } = await form(file(R1, R2, R3))
  await landed(main)
  const card = cardOf(main, CARD)
  const input = textField(card)
  type(input, String.raw`^output\.weight=CPU`)
  fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' })
  await waitFor(() =>
    expect(
      coverageOf(ruleGroup(card, 1))?.textContent?.trim(),
      '21 T.10: Enter rebuilt the rows, and the line with them',
    ).toBe(one('512.0 MiB')),
  )
  expect(coverageTexts(card, 1)[0], 'the output head, 512 MiB').toBe(
    one('512.0 MiB'),
  )
})

test('21 T.15: a Custom rule line follows every edit of its Pattern', async () => {
  const { main } = await form(file(`${HAND}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const input = patternOf(ruleGroup(card, 1))
  expect(input, 'the hand-written rule has a Pattern input').not.toBeNull()
  type(input!, String.raw`^output\.weight`)
  await waitFor(() =>
    expect(
      coverageOf(ruleGroup(card, 1))?.textContent?.trim(),
      'the line counts the pattern as typed',
    ).toBe(one('512.0 MiB')),
  )
})

test('21 T.15: the texts on the page: many, many with more taken, unreadable, none, all taken', async () => {
  const rules = [
    String.raw`\.ffn_up_exps`,
    HAND,
    '[',
    'nothing',
    String.raw`blk\.10\.ffn_up_exps`,
  ]
  const { main } = await form(file(...rules))
  await landed(main)
  const card = cardOf(main, CARD)
  const texts = coverageTexts(card, rules.length)
  expect(texts[0], 'rule 1: every up expert, 64 of them').toBe(many(64))
  expect(texts[1], 'rule 2: 20 new, 10 taken by rule 1').toBe(
    `${many(20)}${en('model-profile-ot-more-taken', '10')}`,
  )
  expect(texts[2], 'rule 3: the regex does not compile').toBe(
    en('model-profile-ot-unreadable'),
  )
  expect(texts[3], 'rule 4: nothing matches').toBe(
    en('model-profile-ot-matches-none'),
  )
  expect(texts[4], 'rule 5: its one tensor is taken by rule 1').toBe(
    en('model-profile-ot-all-taken'),
  )
})

test('21 T.15: an empty pattern has no line', async () => {
  const { main } = await form({ ...AS_LOADED, [OT]: '=CPU' })
  await landed(main)
  const card = cardOf(main, CARD)
  const group = ruleGroup(card, 1)
  expect(
    patternOf(group)?.value,
    'the rule is Custom with an empty Pattern',
  ).toBe('')
  expect(coverageOf(group), 'no line for an empty pattern').toBeNull()
})

test('21 T.15: the Pattern input names the line last in aria-describedby, the Tensor select when the rule has no Pattern input, and the line is no live region', async () => {
  const { main } = await form(file(R2, R3))
  await landed(main)
  const card = cardOf(main, CARD)
  // Rule 1 is the embedding table: no Pattern input, so the Tensor select.
  const embd = ruleGroup(card, 1)
  const embdLine = coverageOf(embd)
  expect(embdLine, 'rule 1 has its line').not.toBeNull()
  expect(
    describedLast(tensorOf(embd)),
    'the Tensor select names the line last',
  ).toBe(embdLine!.id)
  // Rule 2 is Custom: the Pattern input.
  const custom = ruleGroup(card, 2)
  const customLine = coverageOf(custom)
  expect(customLine, 'rule 2 has its line').not.toBeNull()
  expect(
    describedLast(patternOf(custom)!),
    'the Pattern input names the line last',
  ).toBe(customLine!.id)
  for (const line of [embdLine, customLine]) {
    expect(line!.getAttribute('aria-live'), 'not a live region').toBeNull()
    expect(
      ['status', 'alert', 'log'].includes(line!.getAttribute('role') ?? ''),
      'no status role',
    ).toBe(false)
  }
})

// ── 21 T.16: no list, a cut list ──────────────────────────────────────────

test('21 T.16: while the facts read is pending there is no line, and the lines arrive with the list', async () => {
  const h = held()
  const { main } = await form(file(R1, R2, R3), { modelInfo: h.answer })
  await landed(main)
  const card = cardOf(main, CARD)
  expect(coverageTexts(card, 3), 'pending: no line').toEqual([null, null, null])
  h.resolve(reply(200, facts()))
  await waitFor(() =>
    expect(
      coverageOf(ruleGroup(card, 1))?.textContent?.trim(),
      'the list arrives: the lines arrive',
    ).toBe(many(12)),
  )
})

test('21 T.16: a failed read, a reason and an empty list all give no line', async () => {
  const failed = await form(file(R1, R2, R3), {
    modelInfo: () => Promise.reject(new TypeError('down')),
  })
  await landed(failed.main)
  expect(
    coverageTexts(cardOf(failed.main, CARD), 3),
    'a failed read: no line',
  ).toEqual([null, null, null])
  cleanup()

  const reason = await form(file(R1, R2, R3), {
    modelInfo: {
      model: MODEL_FILE,
      info: null,
      reason: { key: 'model-info-missing', args: [MODEL_FILE] },
    },
  })
  await landed(reason.main)
  expect(
    coverageTexts(cardOf(reason.main, CARD), 3),
    'a reason: no line',
  ).toEqual([null, null, null])
  cleanup()

  const empty = await form(file(R1, R2, R3), {
    modelInfo: facts({ tensors: [] }),
  })
  await landed(empty.main)
  expect(
    coverageTexts(cardOf(empty.main, CARD), 3),
    'an empty list: no line',
  ).toEqual([null, null, null])
})

test('21 T.14: a tensors of a bad shape is no tensor list: no line, and the layer facts still count', async () => {
  const { main } = await form(file(R1, R2, R3), {
    modelInfo: facts({ tensors: 'token_embd.weight' }),
  })
  await landed(main)
  expect(
    coverageTexts(cardOf(main, CARD), 3),
    'no line for a list of a bad shape',
  ).toEqual([null, null, null])
  expect(
    within(cardOf(main, en('model-profile-gpu-distribution'))).getByText(
      en('model-profile-model-facts-moe', `${BLOCKS}`, '128', '8'),
    ),
    '21 T.14: the read is not failed by the list',
  ).toBeTruthy()
})

test('21 T.16: tensorsTruncated true puts model-profile-ot-truncated with the row count right after the format paragraph, in the wrapper div, and the counts cover the rows received', async () => {
  const cut = tensorList().slice(0, 2)
  const { main } = await form(file(R1, R2, R3), {
    modelInfo: facts({ tensors: cut, tensorsTruncated: true }),
  })
  await landed(main)
  const fieldset = rulesFieldset(cardOf(main, CARD))
  const wrapper = wrapperOf(fieldset)
  const format = document.getElementById(FORMAT)
  expect(format, 'the format paragraph is there').not.toBeNull()
  expect(wrapper.contains(format), 'it is in the wrapper div').toBe(true)
  const line = format!.nextElementSibling
  expect(line, 'the cut line is right after it').not.toBeNull()
  expect(line!.tagName, 'a paragraph').toBe('P')
  expect(line!.parentElement, 'the wrapper div is its parent').toBe(wrapper)
  expect(line!.textContent!.trim(), 'the text names the rows received').toBe(
    en('model-profile-ot-truncated', `${cut.length}`),
  )
  expect(
    coverageTexts(cardOf(main, CARD), 3),
    'the counts cover the rows',
  ).toEqual([
    en('model-profile-ot-matches-none'),
    one('1.21 GiB'),
    en('model-profile-ot-matches-none'),
  ])
})

test('21 T.16: a list that is not cut shows no cut line', async () => {
  const { main } = await form(file(R1, R2, R3))
  await landed(main)
  const card = cardOf(main, CARD)
  // The text's own opening, read off the strings: no paragraph of the card
  // carries it, at any count.
  const text = en('model-profile-ot-truncated', '999')
  const opening = text.slice(0, text.indexOf('999'))
  const paras = [...card.querySelectorAll('p')].filter((p) =>
    (p.textContent ?? '').includes(opening),
  )
  expect(paras, 'no cut line').toHaveLength(0)
})

// ── 21 T.14a: the list comes from the read the page already makes ─────────

test('21 T.14a: the card reuses the facts the page reads: one model-info request for the page, and the lines from it', async () => {
  const { bff, main } = await form(file(R1, R2, R3))
  await landed(main)
  const card = cardOf(main, CARD)
  await waitFor(() =>
    expect(
      coverageOf(ruleGroup(card, 2))?.textContent?.trim(),
      'the lines are drawn',
    ).toBe(one('1.21 GiB')),
  )
  await quiet(bff)
  expect(
    bff.to('GET', INFO),
    '21 T.14a: no second read for the card',
  ).toHaveLength(1)
  expect(bff.to('PUT', PUT), 'and the page saved nothing').toHaveLength(0)
})

// ── 21 T.17: the strings, and the ux state ────────────────────────────────

test('21 T.17: the strings file has TP3 keys, with the brief English', () => {
  const keys = [
    'model-profile-ot-tensors-one',
    'model-profile-ot-tensors-many',
    'model-profile-ot-matches-none',
    'model-profile-ot-all-taken',
    'model-profile-ot-more-taken',
    'model-profile-ot-unreadable',
    'model-profile-ot-truncated',
  ]
  const json = strings as Record<string, string>
  for (const key of keys)
    expect(json[key], `${key}: the strings file has it`).toBe(en(key))
})

test('21 T.17a: the ux check keeps the states it had (no new state), and its stub answers the tensor list', () => {
  const check = readFileSync(
    join(import.meta.dirname, '../ux/check.js'),
    'utf8',
  )
  const stub = readFileSync(join(import.meta.dirname, '../ux/stub.py'), 'utf8')
  const names = [...check.matchAll(/name: '([a-z0-9-]+)'/g)].map((m) => m[1]!)
  expect(
    names,
    '21 T.17a: the placement state is the one that measures this',
  ).toContain('model-profile-placement')
  expect(
    names.filter((n) => /tensor|coverage/.test(n)),
    '21 T.17a: TP3 adds no state of its own',
  ).toEqual([])
  expect(stub, '21 T.17a: the stub list rides on the facts').toContain(
    'tensors',
  )
  expect(stub, '21 T.17a: and the cut flag').toContain('tensorsTruncated')
})
