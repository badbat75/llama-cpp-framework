// Brief 21, sub-slice TP1b (fe/briefs/21-tensor-placement.md), criteria 21 T.18
// to 21 T.23: the rule row's layout (the label above its control, the row
// aligned at the bottom), the two icon buttons (the x that removes a rule, the
// + that adds one), their sizes, and the icons under forced colours. The
// helpers are those of `TensorPlacement1.test.tsx`, repeated here (a test file
// imports no other test file), and so is the stylesheet reader (a small local
// copy, as in every sheet test): most of this slice is a look, and jsdom lays
// nothing out, so the sheet is what pins it. The measurement half of 21 T.23
// (no sideways scroll, no item past its `li`, one item per line at 320 px, at
// 1280 / 760 / 320 px, 100 % and 200 % text, light and dark) is the designer's:
// the `fe/ux` state `model-profile-placement` measures it.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  before,
  DEVICES,
  en,
  fakeBff,
  MODEL_FILE,
  modelProfileDoc,
  reply,
  startAt,
  type DeviceProbe,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const CARD = en('model-profile-tensor-placement')
const RULES = en('model-profile-ot-rules')
const NONE = en('model-profile-ot-none')
const ADD = en('model-profile-ot-add')
/** 21 T.20: `model-profile-ot-remove` carries the rule's legend, so the name is
 *  "Remove Rule N". */
const removeName = (n: number) =>
  en('model-profile-ot-remove', en('model-profile-ot-rule', `${n}`))
/** 21 T.21: the Add button's id is unchanged. */
const ADD_ID = 'model-profile-ot-add'
/** The key this card owns. */
const OT = 'override-tensor'
const EMBD = String.raw`^token_embd\.weight`
const EXPS = String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`
/** A hand-written pattern no kind owns: it selects Custom, so the row has a
 *  Pattern item (21 T.19 names the four items). */
const HAND = String.raw`blk\.1[0-9]\..*_exps`
/** The four items of a rule's row (21 T.19). */
const ITEM_CLASSES = ['.ot-tensor', '.ot-layers', '.ot-goes-on', '.ot-pattern']

const AS_LOADED: Record<string, string> = {
  model: MODEL_FILE,
  'n-gpu-layers': '29',
}

/** `AS_LOADED` with these rules. */
function file(...rules: string[]): Record<string, string> {
  return { ...AS_LOADED, [OT]: rules.join(',') }
}

function facts(): ModelInfo {
  return {
    model: MODEL_FILE,
    info: { blockCount: 33, moe: true, expertCount: 128, expertUsed: 8 },
    reason: null,
  }
}

beforeEach(() => {
  startAt(HASH)
  Element.prototype.scrollIntoView = () => {}
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Waits for the page read with the file `values`. */
async function form(
  values: Record<string, string> = AS_LOADED,
  options: Parameters<typeof fakeBff>[0] = {},
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

/** The moment the as-of line shows for `probedAt` (GD1.7): the read's end. */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The card's read is over: the GPU card shows the probe, so the Tensor
 *  placement card is drawn with the devices it offers. */
async function landed(main: HTMLElement, probe: DeviceProbe = DEVICES) {
  const fs = await within(main).findByRole('group', {
    name: en('model-profile-gpus'),
  })
  if ((probe.devices ?? []).length === 0) {
    await within(fs).findByText(en('model-profile-gpu-none-found'))
    return
  }
  if (typeof probe.probedAt === 'string') {
    await within(fs).findByText(asOfDate(probe.probedAt))
    return
  }
  await within(fs).findByText(en('model-profile-gpu-no-probe'))
}

function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The accessible name of a control, as the page gives it. */
function nameOf(el: HTMLElement): string {
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const label = el.getAttribute('aria-label')
  if (label) return label.replace(/\s+/g, ' ').trim()
  const id = el.getAttribute('id')
  const tied = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (tied?.textContent ?? wrap?.textContent ?? el.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

function ruleGroup(card: HTMLElement, n: number): HTMLElement {
  return within(card).getByRole('group', {
    name: en('model-profile-ot-rule', `${n}`),
  })
}

/** The rule's wrapping row (the coder's class, `.ot-rule-row`). */
function rowOf(group: HTMLElement): HTMLElement {
  const row = group.querySelector('[class*="ot-rule-row"]')
  expect(row, 'the rule has its row').not.toBeNull()
  return row as HTMLElement
}

function removeOf(group: HTMLElement, n: number): HTMLElement {
  return within(group).getByRole('button', { name: removeName(n) })
}

function addOf(card: HTMLElement): HTMLElement {
  return within(card).getByRole('button', { name: ADD })
}

/** The Rules fieldset and the wrapper `div` inside it (21 T.2). */
function rulesFieldset(card: HTMLElement): HTMLElement {
  return within(card).getByRole('group', { name: RULES })
}

function wrapperOf(fieldset: HTMLElement): HTMLElement {
  const div = fieldset.querySelector(':scope > div')
  expect(div, 'the Rules fieldset has its wrapper div').not.toBeNull()
  return div as HTMLElement
}

/** The text a sighted user reads inside `el`: what is not an aria-hidden icon
 *  and not a `visually-hidden` span (21 T.20, 21 T.21: "no visible text"). */
function visibleText(el: HTMLElement): string {
  return [...el.childNodes]
    .filter(
      (n) =>
        !(n instanceof Element && n.getAttribute('aria-hidden') === 'true') &&
        !(n instanceof Element && n.classList.contains('visually-hidden')),
    )
    .map((n) => n.textContent ?? '')
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The `span.visually-hidden` that gives `button` its name (21 T.20, 21 T.21). */
function hiddenSpan(button: HTMLElement): HTMLElement {
  const spans = [...button.querySelectorAll('span')].filter((s) =>
    s.classList.contains('visually-hidden'),
  )
  expect(
    spans.length,
    'the button has exactly one visually hidden span',
  ).toBeGreaterThan(0)
  return spans[0]!
}

/** The button's icon: an `aria-hidden` svg (21 T.20, 21 T.21). */
function iconOf(button: HTMLElement): Element {
  const icon = button.querySelector('svg[aria-hidden="true"]')
  expect(icon, 'the button has an aria-hidden icon').not.toBeNull()
  return icon!
}

// ── The stylesheet reader (the way `Reflow.test.ts` reads `index.css`) ─────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  body: string
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const SHEET_RULES: (Rule & { depth: number })[] = [
  ...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g),
].map((m) => ({
  sel: m[1].trim(),
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the layout applies at every width, not in a media block.
const TOP = SHEET_RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length
          ? ` (${list.map((r) => r.sel.replace(/\s+/g, ' ')).join(' | ')})`
          : ''),
    )
  return list[0]
}

// The top-level rule styling this selector, as one part of a comma group: the
// sheet's own style for a shared declaration, so the coder's one rule for two
// items is as good as two.
const topStyling = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) =>
      r.sel
        .split(',')
        .some((p) => p.replace(/\s+/g, '') === sel.replace(/\s+/g, '')),
    ),
    `the top-level rule styling "${sel}" (${what})`,
  )

/** The declarations that reach a `button` carrying `classes`, in the order the
 *  cascade applies them: the top-level rules whose selector is `button` or a
 *  compound of those classes (`.secondary`, `.secondary.ot-add`), ordered by
 *  specificity (the classes it counts) and then by the sheet's order. A
 *  selector with a descendant, a pseudo-class or an attribute is skipped: it
 *  needs a tree jsdom does not lay out, and the brief gives these two buttons
 *  their look under a class of their own. The classes are read off the page,
 *  so the pin follows the names the coder chose. */
function cascade(classes: string[]): Record<string, string> {
  const own = new Set(['button', ...classes.map((c) => `.${c}`)])
  const hits: { rule: Rule; spec: number; at: number }[] = []
  TOP.forEach((rule, at) => {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const tokens = part.match(/\.[-a-z0-9_]+|[-a-z0-9_]+/g) ?? []
      if (tokens.length > 0 && tokens.every((t) => own.has(t)))
        spec = Math.max(spec, tokens.filter((t) => t.startsWith('.')).length)
    }
    if (spec >= 0) hits.push({ rule, spec, at })
  })
  hits.sort((a, b) => a.spec - b.spec || a.at - b.at)
  const out: Record<string, string> = {}
  for (const { rule } of hits)
    for (const d of rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return out
}

/** The rules inside every `@media` block whose prelude matches `what`: the
 *  block is brace-matched, its rules read with the sheet's own regex. */
function mediaRules(what: RegExp): Rule[] {
  const out: Rule[] = []
  for (const m of CSS.matchAll(/@media[^{]+\{/g)) {
    if (!what.test(m[0])) continue
    const start = (m.index ?? 0) + m[0].length
    let depth = 1
    let i = start
    while (i < CSS.length && depth > 0) {
      if (CSS[i] === '{') depth++
      else if (CSS[i] === '}') depth--
      i++
    }
    for (const r of CSS.slice(start, i - 1).matchAll(/([^{}]+)\{([^{}]*)\}/g))
      out.push({
        sel: r[1].trim(),
        compact: r[1].replace(/\s+/g, ''),
        body: r[2],
      })
  }
  return out
}

// ── 21 T.18: "Rules" is a field label ─────────────────────────────────────

test('21 T.18: "Rules" is a legend.field-label, carrying every class the "Rules as text" label carries, and the sheet draws the two the same way', async () => {
  // The user: "Rules" is drawn as plain text while "Rules as text" is a bold
  // field label, which is what made the indent look wrong. The label is the
  // same element class the form's fields use, so the same declarations reach
  // it, and `legend.field-label` floats it so it lays out in the label column.
  const { main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  const card = cardOf(main, CARD)
  const fieldset = rulesFieldset(card)
  const legend = fieldset.querySelector(':scope > legend')
  expect(legend, '21 T.18: the fieldset has its legend').not.toBeNull()
  expect(
    legend!.classList.contains('field-label'),
    '21 T.18: the legend carries the field label class',
  ).toBe(true)
  // The fieldset's accessible name is its legend: `rulesFieldset` above found
  // the group by the name "Rules", which pins the naming (21 T.3). The local
  // `nameOf` reads labels and aria attributes, not a legend, so the pin here
  // is the legend's own text.
  expect(
    legend!.textContent?.replace(/\s+/g, ' ').trim(),
    '21 T.18: the legend text is the key, and the group was found by that name',
  ).toBe(RULES)
  // The "Rules as text" Field's label: the `div.field-label` of the TP2 text
  // field. Every class it carries is on the legend, so every declaration that
  // draws it draws the legend.
  const text = within(card).getByRole('textbox', {
    name: en('model-profile-ot-text'),
  })
  const label = text.closest('.field')?.querySelector('.field-label')
  expect(label, '21 T.18: Rules as text has its field label').not.toBeNull()
  for (const cls of [...(label as HTMLElement).classList])
    expect(
      legend!.classList.contains(cls),
      `21 T.18: the legend carries "${cls}", the class that draws the "Rules as text" label`,
    ).toBe(true)
  // The sheet: the label's own declarations, and the legend's float.
  const fieldLabel = topStyling('.field-label', 'the field label of a Field')
  for (const [name, value] of [
    ['display', 'flex'],
    ['font-weight', '600'],
  ] as Array<[string, string]>)
    expect(
      norm(prop(fieldLabel.body, name)),
      `21 T.18: .field-label: the label of "Rules as text" is drawn this way, so the legend is too: ${name}`,
    ).toBe(value)
  const floated = topStyling(
    'legend.field-label',
    'the Rules legend in the label column',
  )
  expect(
    norm(prop(floated.body, 'float')),
    '21 T.18: legend.field-label: floated, so the fieldset indents like a field',
  ).toBe('left')
})

// ── 21 T.19: the label above its control, the row at the bottom ───────────

test('21 T.19: in a rule row every item holds its label then its control, and the sheet makes each item a column (display: flex, flex-direction: column) and the row align-items: flex-end', async () => {
  // The user: "Tensor" sits beside its select while "Goes on" sits above its
  // own, so the two selects stand at different heights. The fix: every item is
  // a column, so the label is always above, and the row is aligned at the
  // bottom, so the controls of one line share a top edge. jsdom pins the
  // declarations; `fe/ux` measures the tops.
  // Layers shows only for a kind with a block and Pattern only for Custom
  // (21 T.4), so no single row holds all four items: the fixture is one rule
  // of a block kind (Tensor, Layers, Goes on) and one Custom rule (Tensor,
  // Goes on, Pattern), and the four items are pinned across the two rows.
  const { main } = await form(file(`${EXPS}=CPU`, `${HAND}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const rows = [rowOf(ruleGroup(card, 1)), rowOf(ruleGroup(card, 2))]
  for (const cls of ITEM_CLASSES) {
    const item = rows
      .map((row) => row.querySelector(`[class*="${cls.slice(1)}"]`))
      .find((el) => el !== null)
    expect(item, `21 T.19: one row has its ${cls} item`).not.toBeNull()
    const label = item!.querySelector('label')
    const control = item!.querySelector('select, input')
    expect(label, `21 T.19: ${cls}: the item holds its label`).not.toBeNull()
    expect(
      control,
      `21 T.19: ${cls}: the item holds its control`,
    ).not.toBeNull()
    expect(
      before(label!, control!),
      `21 T.19: ${cls}: the label comes before the control, so a column puts it above`,
    ).toBe(true)
    expect(
      control!.parentElement,
      `21 T.19: ${cls}: the control is a direct item of the item, so the column sizes it`,
    ).toBe(item)
    const rule = topStyling(cls, 'an item of a rule row')
    for (const [name, value] of [
      ['display', 'flex'],
      ['flex-direction', 'column'],
    ] as Array<[string, string]>)
      expect(
        norm(prop(rule.body, name)),
        `21 T.19: ${cls}: the item is a column, its label above its control: ${name}`,
      ).toBe(value)
  }
  const itemsOf = (row: HTMLElement) =>
    [...row.children]
      .map((el) =>
        ITEM_CLASSES.find((cls) => el.classList.contains(cls.slice(1))),
      )
      .filter((cls): cls is string => cls !== undefined)
  expect(
    itemsOf(rows[0]!),
    '21 T.19: the block-kind row holds Tensor, Layers and Goes on (21 T.4: Layers only for a kind with a block)',
  ).toEqual(['.ot-tensor', '.ot-layers', '.ot-goes-on'])
  expect(
    itemsOf(rows[1]!),
    '21 T.19: the Custom row holds Tensor, Goes on and Pattern (21 T.4: Pattern only for Custom)',
  ).toEqual(['.ot-tensor', '.ot-goes-on', '.ot-pattern'])
  const rowRule = topStyling('.ot-rule-row', 'the row of a rule')
  expect(
    norm(prop(rowRule.body, 'align-items')),
    '21 T.19: the row is aligned at the bottom, so every control of a line shares its top edge',
  ).toBe('flex-end')
})

// ── 21 T.20: Remove is an x icon button ───────────────────────────────────

test('21 T.20: Remove is the x icon and a visually hidden span filled with "Rule 2", so its name is "Remove Rule 2", it shows no text, and it is the last item of the row', async () => {
  // The user: "a pill button labelled Remove is not needed when it sits on the
  // same row: an x is enough". The name stays the words of
  // `model-profile-ot-remove` with the rule's legend, now in a span the sighted
  // user does not read (WCAG 2.5.3 is satisfied by the icon alone: the x is
  // the whole of the visible control, as the chips' x already is).
  const { main } = await form(file(`${EMBD}=CUDA0`, `${EXPS}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const group = ruleGroup(card, 2)
  const row = rowOf(group)
  const button = removeOf(group, 2)
  expect(
    nameOf(button),
    '21 T.20: the name is model-profile-ot-remove filled with the legend',
  ).toBe(removeName(2))
  expect(
    hiddenSpan(button).textContent?.replace(/\s+/g, ' ').trim(),
    '21 T.20: the span holds the words, filled with "Rule 2"',
  ).toBe(removeName(2))
  expect(visibleText(button), '21 T.20: the button shows no text').toBe('')
  expect(
    iconOf(button).classList.contains('icon'),
    '21 T.20: the icon is the sheet-sized icon',
  ).toBe(true)
  expect(
    button.getAttribute('aria-label'),
    '21 T.20: the name comes from the span, not from an aria-label',
  ).toBeNull()
  expect(
    button.parentElement,
    '21 T.20: Remove is a direct item of the row',
  ).toBe(row)
  for (const cls of ITEM_CLASSES) {
    const item = row.querySelector(`[class*="${cls.slice(1)}"]`)
    if (item)
      expect(
        before(item, button),
        `21 T.20: Remove is after the ${cls} item, so it is the last item of the row`,
      ).toBe(true)
  }
  // 21 T.20: focus after a removal is unchanged (21 T.7). The row's own
  // removal focus is pinned there; the button's shape cannot change it.
})

test('21 T.20 (the sheet): Remove is 40 px wide and at least 40 px high, with no border and no fill, and align-self: flex-end', async () => {
  // 21 T.20, 21 T.22. The cascade is read for the classes the button carries,
  // so the pin is the look (a borderless, fillless 40 px square sitting on the
  // row's bottom edge), not the coder's exact declarations.
  const { main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  const button = removeOf(ruleGroup(cardOf(main, CARD), 1), 1)
  const at = cascade([...button.classList])
  expect(
    norm(at['width'] ?? ''),
    '21 T.20: the x is 40 px wide, so it lines up with the selects',
  ).toBe('40px')
  expect(
    norm(at['min-height'] ?? ''),
    '21 T.20: and at least 40 px high (2.5.8 asks 24; the row is 40)',
  ).toBe('40px')
  const borderless = ['none', '0'].includes(norm(at['border'] ?? ''))
  expect(
    borderless ||
      norm(at['border-style'] ?? '') === 'none' ||
      norm(at['border-width'] ?? '') === '0',
    '21 T.20: no border: the x is not a pill',
  ).toBe(true)
  expect(
    ['none', 'transparent'].includes(norm(at['background'] ?? '')) ||
      ['none', 'transparent'].includes(norm(at['background-color'] ?? '')),
    '21 T.20: no fill',
  ).toBe(true)
  expect(
    norm(at['align-self'] ?? ''),
    '21 T.22: Remove is align-self: flex-end, so it stands on the row bottom',
  ).toBe('flex-end')
})

// ── 21 T.21: Add is a plus icon button ────────────────────────────────────

test('21 T.21: Add is the plus icon and a visually hidden span, so its name is "Add rule", it shows no text, its id is unchanged, and it is the last thing in the wrapper div, after the list and after the empty line', async () => {
  // The user: "Add too: a + in the row below is enough". It keeps an outline
  // (the sheet test) so after the empty-list line it still reads as a control.
  const { main } = await form(file(`${EMBD}=CUDA0`, `${EXPS}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const button = addOf(card)
  const wrapper = wrapperOf(rulesFieldset(card))
  expect(nameOf(button), '21 T.21: the name is unchanged').toBe(ADD)
  expect(
    hiddenSpan(button).textContent?.replace(/\s+/g, ' ').trim(),
    '21 T.21: the span holds model-profile-ot-add',
  ).toBe(ADD)
  expect(visibleText(button), '21 T.21: the button shows no text').toBe('')
  expect(
    iconOf(button).classList.contains('icon'),
    '21 T.21: the icon is the sheet-sized icon',
  ).toBe(true)
  expect(button.getAttribute('aria-label'), '21 T.21: no aria-label').toBeNull()
  expect(button.id, '21 T.21: the id is unchanged').toBe(ADD_ID)
  expect(
    button.parentElement,
    '21 T.21: Add is inside the wrapper div, as it is today',
  ).toBe(wrapper)
  const list = within(card).getByRole('list')
  expect(before(list, button), '21 T.21: Add is after the list').toBe(true)
  expect(
    wrapper.lastElementChild,
    '21 T.21: and before the end of the wrapper div, so it is its last child',
  ).toBe(button)
  // The empty list: the line of `model-profile-ot-none` stands in for the
  // list, and Add follows it (the same place, so the tab order of 21 T.13
  // holds in both cases).
  cleanup()
  const empty = await form({ ...AS_LOADED })
  await landed(empty.main)
  const emptyCard = cardOf(empty.main, CARD)
  const add = addOf(emptyCard)
  const box = wrapperOf(rulesFieldset(emptyCard))
  const line = within(emptyCard).getByText(NONE)
  expect(
    before(line, add),
    '21 T.21: with no rule, Add is after the empty line',
  ).toBe(true)
  expect(
    box.lastElementChild,
    '21 T.21: with no rule, Add is still the wrapper div last child',
  ).toBe(add)
})

test('21 T.21 (the sheet): Add is 40 by 40 px with the --control outline', async () => {
  // 21 T.21: a 40 px outlined square, not a bare glyph, "to keep a clear
  // target beside text". The outline is the --control one of a secondary
  // button, so the + reads as a control after the empty-list line.
  const { main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  const at = cascade([...addOf(cardOf(main, CARD)).classList])
  expect(norm(at['width'] ?? ''), '21 T.21: the + is 40 px wide').toBe('40px')
  expect(norm(at['min-height'] ?? ''), '21 T.21: and 40 px high').toBe('40px')
  expect(norm(at['border'] ?? ''), '21 T.21: with the --control outline').toBe(
    'var(--border-w) solid var(--control)',
  )
})

// ── TP1c: 21 T.24, the room above the + ───────────────────────────────────

test('21 T.24: the Add + declares margin-top: var(--space-3), a room below the --space-4 that separates two rules, and nothing else about it moves', async () => {
  // The user, on the deployed build: "il bottone + è attaccato alla riga
  // sopra". `ul.check-list` has `margin: 0` and `.ot-add` carries no margin of
  // its own, so the + sits on the last rule's bottom edge. One token of room,
  // one step below the space between two rules, so the + reads as the list's
  // own control and not as another rule. The same rule reaches the + after the
  // `model-profile-ot-none` line, where there is no rule. The measure in the
  // three browsers is the designer's, at the review; jsdom pins the sheet.
  const { main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  const at = cascade([...addOf(cardOf(main, CARD)).classList])
  expect(
    norm(at['margin-top'] ?? ''),
    '21 T.24: the room above the +, as a token, not a literal',
  ).toBe('var(--space-3)')
  // The room is smaller than the one between two rules, which is the point of
  // the preference: the + belongs to the list, it is not a rule of its own.
  const rules = topStyling(':root', 'the tokens')
  const space = (name: string) => parseInt(prop(rules.body, name), 10)
  expect(
    norm(
      prop(
        topStyling('#model-profile-ot-rules li + li', 'D3').body,
        'margin-top',
      ),
    ),
    '21 T.24: the space between two rules, the one the + must stay under',
  ).toBe('var(--space-4)')
  expect(
    space('--space-3') < space('--space-4'),
    `21 T.24: --space-3 (${space('--space-3')}px) is less than --space-4 (${space('--space-4')}px)`,
  ).toBe(true)
  // Nothing else moves: the square of 21 T.21 is as it was, and the rule adds
  // no other space around the +.
  expect(norm(at['width'] ?? ''), '21 T.24: the + is still 40 px wide').toBe(
    '40px',
  )
  expect(norm(at['min-height'] ?? ''), '21 T.24: and still 40 px high').toBe(
    '40px',
  )
  expect(
    norm(at['border'] ?? ''),
    '21 T.24: and still the --control outline',
  ).toBe('var(--border-w) solid var(--control)')
  expect(norm(at['padding'] ?? ''), '21 T.24: its padding is untouched').toBe(
    '0',
  )
  for (const name of [
    'margin',
    'margin-bottom',
    'margin-left',
    'margin-right',
    'margin-inline',
    'margin-block',
  ])
    expect(
      norm(at[name] ?? ''),
      `21 T.24: nothing else moves, so the + declares no ${name}`,
    ).toBe('')
})

// ── TP1c review (the designer, 2026-11-24): D1, the + after the empty line ──

test('21 T.24: after the empty-list line the + has no room of its own, so the gap is the paragraph’s 12 px and not 24 (TP1c review D1)', async () => {
  // The + is an inline-flex box, so its margin-top never collapses with the
  // bottom margin the global `p` rule gives the `model-profile-ot-none` line
  // (index.css 314): after the empty line the + sits 12 + 12 = 24 px below it,
  // twice the room of 21 T.24 (the review, D1). The fix the review names is a
  // rule for a `.ot-add` right after a `p`, whose specificity (0,1,1) beats
  // `.ot-add`’s (0,1,0), so the paragraph’s own 12 px is the gap. The measure
  // in the three browsers is the designer’s re-check; jsdom pins the shape and
  // the sheet.
  const empty = await form({ ...AS_LOADED })
  await landed(empty.main)
  const card = cardOf(empty.main, CARD)
  const add = addOf(card)
  const line = within(card).getByText(NONE)
  expect(
    line.tagName,
    '21 T.24: the empty-list line is a p, whose bottom margin is the gap',
  ).toBe('P')
  expect(
    add.previousElementSibling,
    '21 T.24: the + sits right after that paragraph, where the rule reaches it',
  ).toBe(line)
  const after = TOP.filter((r) =>
    r.sel.split(',').some((p) => /^p\s*\+\s*\.ot-add$/.test(p.trim())),
  )
  expect(
    after.length,
    'D1: the sheet has one rule for a .ot-add right after a p (the review names it: p + .ot-add { margin-top: 0 })',
  ).toBe(1)
  expect(
    norm(prop(after[0]!.body, 'margin-top')),
    'D1: there the + declares no room of its own, so the paragraph’s 12 px is the gap',
  ).toBe('0')
  // After rules the + follows the list and not a paragraph, so the rule leaves
  // it the --space-3 room of 21 T.24 (the cascade test above pins that room).
  cleanup()
  const withRule = await form(file(`${EMBD}=CUDA0`))
  await landed(withRule.main)
  expect(
    addOf(cardOf(withRule.main, CARD)).previousElementSibling?.tagName,
    '21 T.24: after rules the + follows the ul, not a p, so it keeps its room',
  ).toBe('UL')
})

// ── 21 T.22: the sizes of the line ────────────────────────────────────────

test('21 T.22 (as its TP1b review D2 amends it): Tensor is flex 2 1 18rem, Goes on flex 2 1 16rem, Layers flex 0 1 8rem, all three min-width: 0, and their controls width: 100%', async () => {
  // The user: "the selects cut their text". The two selects get more of the
  // line and the Layers field a fixed 8rem; each control fills its item, which
  // is now a column, so `width: 100%` is its width (a flex basis in a column
  // would be its height).
  // The TP1b review (D2, 2026-11-23) retargets the Layers share: the fixed
  // `0 0 8rem` of TP1b cannot shrink, so at 200 % text it is 256 px wide in a
  // 190 px column (1280 px window) and spills its rule by 18 to 66 px in all
  // three engines (tp1/run5.txt). The item keeps the 8rem basis and shrinks.
  const { main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  for (const [sel, flex] of [
    ['.ot-tensor', '2 1 18rem'],
    ['.ot-goes-on', '2 1 16rem'],
    ['.ot-layers', '0 1 8rem'],
  ] as Array<[string, string]>) {
    const rule = topStyling(sel, 'an item of a rule row')
    expect(
      norm(prop(rule.body, 'flex')),
      `21 T.22: ${sel}: its share of the line`,
    ).toBe(flex)
    expect(
      norm(prop(rule.body, 'min-width')),
      `21 T.22: ${sel}: a flex item is floored at its min-content, so it needs the floor to shrink (1.4.10 at 320 px)`,
    ).toBe('0')
  }
  for (const sel of [
    '.ot-tensor select',
    '.ot-goes-on select',
    '.ot-layers input',
  ]) {
    const rule = topStyling(sel, 'the control of an item of a rule row')
    expect(
      norm(prop(rule.body, 'width')),
      `21 T.22: ${sel}: the control fills its item, so the text it shows is the width the item has`,
    ).toBe('100%')
  }
})

// ── TP1b review (the designer, 2026-11-23): D1, the controls' shared flex ───

test('21 T.19 (TP1b review D1): .ot-rule-row input and .ot-rule-row select declare flex: none, so the shared input basis is not a height inside the column items, and the Pattern input takes its width from width: 100%', () => {
  // The shared `input[type=text], input[type=password] { flex: 1 1 16rem }`
  // (index.css 1572) also applies inside the column items of 21 T.19, where a
  // flex basis is the HEIGHT: the Layers input stands 110 px tall and the
  // Pattern input 256 px (512 at 200 %), and the row aligned at the bottom
  // lifts the Tensor select 70 px under the Layers label: the misalignment the
  // user reported, on another field (tp1b-chromium-1600-light.png). The
  // review's fix switches the flex off for every control of a rule row; the
  // width then comes from the `width: 100%` the controls carry (21 T.22, and
  // the Pattern input pinned below, the other half of the fix). The browser
  // measure (every control of a line 40 px tall, 42 in WebKit, sharing one
  // top) is the review's; jsdom pins the declarations.
  const parts = TOP.filter(
    (r) => norm(prop(r.body, 'flex')) === 'none',
  ).flatMap((r) => r.sel.split(',').map((p) => p.replace(/\s+/g, ' ').trim()))
  for (const what of ['input', 'select'])
    expect(
      parts.some((p) => new RegExp(`^\\.ot-rule-row\\s+${what}$`).test(p)),
      `D1: .ot-rule-row ${what}: the shared flex basis is a height in a column item, so no control of a rule row may keep it (the review names the two selectors as one comma group, the sheet's style for a shared declaration)`,
    ).toBe(true)
  const patternInput = topStyling(
    '.ot-pattern input',
    'the Pattern input of a rule row',
  )
  expect(
    norm(prop(patternInput.body, 'width')),
    'D1: with the flex switched off, the Pattern input is as wide as its line because of this: width',
  ).toBe('100%')
})

// ── TP1b review (the designer, 2026-11-23): D3, the space between rules ─────

test('21 T.3 (TP1b review D3): #model-profile-ot-rules li + li declares margin-top: var(--space-4), so a rule legend never reads as a line of the rule above', () => {
  // The rules sit with no space between them, so "Rule 2" reads as a line of
  // rule 1 just under its x (the review, D3). The list is the `ul role="list"`
  // of 21 T.3 inside the Rules fieldset, whose id the sheet already names (the
  // 720 px placement rule of the TP1 review D1); the space is one token,
  // --space-4, and it falls between rules only (`li + li`, not the first).
  const spaced = topStyling(
    '#model-profile-ot-rules li + li',
    'the space between two rules',
  )
  expect(
    norm(prop(spaced.body, 'margin-top')),
    'D3: the rules are separated by this: margin-top',
  ).toBe('var(--space-4)')
})

// ── 21 T.23: forced colours ───────────────────────────────────────────────

test('21 T.23: under forced colours both icon buttons keep their glyph visible (color: ButtonText)', async () => {
  // The icons are `currentColor` strokes, so a button whose text colour is a
  // system colour keeps the glyph. The layout half of 21 T.23 (no sideways
  // scroll, no item past its `li`, one item per line at 320 px, at 1280 / 760
  // / 320 px, 100 % and 200 % text, light and dark, with the five long rules)
  // is the designer's measurement: the `fe/ux` state `model-profile-placement`
  // measures it in three engines.
  const { main } = await form(file(`${EMBD}=CUDA0`, `${EXPS}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const forced = mediaRules(/forced-colors/)
  for (const [what, button] of [
    ['Remove', removeOf(ruleGroup(card, 1), 1)],
    ['Add', addOf(card)],
  ] as Array<[string, HTMLElement]>) {
    const classes = [...button.classList].map((c) => `.${c}`)
    const hits = forced.filter((r) =>
      classes.some((c) => r.compact.includes(c)),
    )
    expect(
      hits.length,
      `21 T.23: ${what}: a forced-colours rule styles the button (the icons must stay visible)`,
    ).toBeGreaterThan(0)
    expect(
      hits.some((r) => norm(prop(r.body, 'color')) === 'ButtonText'),
      `21 T.23: ${what}: its colour is ButtonText, so the icon is drawn in the system text colour`,
    ).toBe(true)
  }
})
