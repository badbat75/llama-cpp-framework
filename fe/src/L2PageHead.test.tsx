// Brief 22, slice L2 (fe/briefs/22-model-sources.md, "L2 acceptance list for
// the tester" items 6 and 7): 22 D4.31, the two cards' intro kept short with
// the rest behind the card's About, and 22 D4.32, the page head, whose
// settings heading stays in the accessibility tree as `.visually-hidden`.
//
// The design-review round of 2026-11-27 (fe/briefs/22-model-sources.md, D1 and
// its advice 2) rewrites the item 6 tests: the intro key shrinks to the card's
// first SENTENCE (its first line was 594 characters, so nothing had got
// shorter), the rest of the old string moves to the new `-about` keys, and the
// card's About toggle moves beside the card's h3, in the heading row. The
// words of D1 are pinned as the review writes them, and read from
// `strings/en_US.json`: a pin that reads both sides of the strings mirror
// (test/bff.ts's EN) is a tautology, the coder moving one file keeping it
// green.
//
// The same round's second review (fe/briefs/22-model-sources.md, "D3, advice 2
// built otherwise") is pinned at the end of the advice 2 tests: advice 2 built
// the card's About as a bare verb at the row's end, and the review asks for the
// field About's look, 8 px after the title. It is a sheet rule, so it is pinned
// as the declarations the review takes away, read off the toggle and off a
// field's About of the same card. The measure itself (the toggle's left at the
// title's right plus 8 px) is the UX check's, jsdom laying nothing out.
//
// Not provable in jsdom, so not tested here: the two gaps of D4.32 (the
// switch and its word `--space-2` apart, and the `--space-5` the state line
// leaves before the first card), which are the designer's measurement of the
// UX check's model-profile-page states. The card's own fields, and what the
// page saves, are ModelProfileGpu*.test.tsx and ModelProfileSave*.test.tsx.
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
  isHidden,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const MODEL = '#/profiles/model/Qwen3-27B'
const GPU = en('model-profile-gpu-distribution')
const TENSOR = en('model-profile-tensor-placement')

// The strings file itself, not the test mirror: the words of D1 are pinned
// against what the page reads.
const S = JSON.parse(
  readFileSync(join(import.meta.dirname, 'strings', 'en_US.json'), 'utf8'),
) as Record<string, string>

/** One card of D4.31: its title, its two keys, and the words the review gives
 *  (fe/briefs/22-model-sources.md, D1: the intro is the first sentence, the
 *  about key is the rest of the old string, `\n` breaks included). */
const INTRO_CARDS = [
  {
    title: GPU,
    introKey: 'model-profile-gpu-distribution-intro',
    aboutKey: 'model-profile-gpu-distribution-about',
    intro:
      'Which GPUs run THIS model and how much each holds, saved to models.ini.',
    about:
      "Check nothing to let llama.cpp choose. With two GPUs of different sizes, check both and give the bigger one more; left on Auto, the split follows free VRAM at load, not an even cut. Under split mode layer, once the model file is read, each GPU takes a number of blocks: the model's blocks and its output layer, shared out top to bottom. Otherwise the weights are ratios: 3 and 1 give 75% and 25%. The checked GPUs, top to bottom, are the split order: Move up and Move down change it; the first one is llama.cpp's main GPU.\nKeys: split-mode, device, tensor-split.",
  },
  {
    title: TENSOR,
    introKey: 'model-profile-tensor-placement-intro',
    aboutKey: 'model-profile-tensor-placement-about',
    intro:
      'Exceptions to the GPU distribution above: tensors that go on a device of their own, whatever their layer does.',
    about:
      'On a MoE model, sending the experts of some layers to the CPU frees the most VRAM, as MoE CPU layers does for the first ones. llama.cpp also keeps the embedding table in RAM even when it reports every layer on the GPU: put it on a GPU to reclaim that memory, if the card has room. The first rule that matches a tensor wins.\nKey: override-tensor.',
  },
]

/** The card whose heading is `title`. */
const card = (main: HTMLElement, title: string): HTMLElement =>
  within(main)
    .getByRole('heading', { level: 3, name: title })
    .closest('section')!

/** The card's heading. */
const cardH3 = (main: HTMLElement, title: string): HTMLElement =>
  within(main).getByRole('heading', { level: 3, name: title })

/** The card's About of its own intro: the toggle of D4.31, named as a
 *  field's About is named (Field.tsx's HintToggle), with the card's title for
 *  its label. */
const introToggle = (box: HTMLElement, title: string): HTMLElement =>
  within(box).getByRole('button', { name: `${en('field-about')} ${title}` })

/** The element the card's About toggle opens (its `aria-controls`): the panel
 *  of D4.31, reached by id, so the pin survives the coder naming it a div, a
 *  section or a group. */
const introPanel = (box: HTMLElement, toggle: HTMLElement): HTMLElement => {
  const id = toggle.getAttribute('aria-controls')
  expect(id, 'the toggle names the panel it opens').not.toBeNull()
  const panel = box.querySelector(`#${id}`)
  expect(panel, `the panel ${id} is in the card`).not.toBeNull()
  return panel as HTMLElement
}

/** The paragraphs of `el`, as the page shows their words. */
const paraTexts = (el: HTMLElement): string[] =>
  [...el.querySelectorAll('p')].map((p) => p.textContent ?? '')

/** The controls of `root` the Tab path walks, in document order, without the
 *  `tabindex="-1"` ones (the card headings, 2h.196) and the disabled ones
 *  (HearthSaveBarScroll's reader, a small local copy). */
function stops(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      el.getAttribute('tabindex') !== '-1' &&
      !el.hasAttribute('disabled') &&
      !el.closest('[inert]'),
  )
}

// ── The sheet, read as text (Reflow.test.ts's reader, a small local copy:
// importing that file would register its tests here) ───────────────────────

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

/** Every rule of the sheet that reaches `el`. A selector jsdom's engine cannot
 *  parse is skipped (a `:has()` of another element, a pseudo-element). */
const reaching = (el: Element): typeof RULES =>
  RULES.filter((r) => {
    try {
      return el.matches(r.sel)
    } catch {
      return false
    }
  })

/** Every rule reaching `el` that declares `name`, as "selector: name: value",
 *  so a red names the rule to fix. */
const declaring = (el: Element, name: string): string[] =>
  reaching(el)
    .filter((r) => prop(r.body, name) !== '')
    .map((r) => `${r.sel}: ${name}: ${norm(prop(r.body, name))}`)

/** The shorthand that carries a longhand, and how to read the longhand out of
 *  it: the sheet's `.about` rule writes `padding: 0 var(--space-2)` and
 *  `border: 1px solid var(--control)`, while the rules the second review (D3)
 *  takes away write the longhands. A pin of the longhand that read only the
 *  longhand would be red on the fix the review asks for. */
const CARRIES: Record<string, Array<[string, (v: string) => string]>> = {
  'padding-inline': [['padding', (v) => v.split(/\s+/)[1] ?? '']],
  'border-color': [['border', (v) => v.split(/\s+/).slice(-1)[0] ?? '']],
}

/** The value `el` ends with for `name`: the declarations reaching it, ordered as
 *  the cascade orders them (the reader of ListColumns.test.tsx: the specificity
 *  counted by `.` and `#`, a tie on source order), the last one winning, a
 *  shorthand that carries the longhand read for it. The order matters here:
 *  `.about` sits AFTER `.card-heading .about` in the sheet, and the cascade
 *  gives the more specific rule, not the later one. */
const valueOf = (el: Element, name: string): string => {
  const hits = reaching(el)
    .map((r, i) => {
      const spec = (r.sel.match(/[.#]/g) ?? []).length
      const direct = prop(r.body, name)
      if (direct !== '') return { i, spec, value: direct }
      for (const [short, take] of CARRIES[name] ?? []) {
        const v = prop(r.body, short)
        if (v !== '') return { i, spec, value: take(v) }
      }
      return null
    })
    .filter((h) => h !== null)
    .sort((a, b) => a.spec - b.spec || a.i - b.i)
  return hits.length === 0 ? '' : norm(hits[hits.length - 1].value)
}

/** The model profile page, its cards read: the two intros of D4.31 show. */
async function cardsPage(options: BffOptions = {}): Promise<HTMLElement> {
  startAt(MODEL)
  fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  // The cards come with the read, so the Tensor card is the last of them.
  await within(main).findByRole('heading', { level: 3, name: TENSOR })
  return main
}

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Item 6: 22 D4.31, as its D1 review (2026-11-27) amends it ─────────────

test('22 L2 D4.31 (its D1 review): the strings file keeps each card’s intro to the one sentence the review gives, and holds the rest of the old string in the new -about key', () => {
  for (const c of INTRO_CARDS) {
    expect(
      S[c.introKey],
      `${c.introKey}: the card’s intro is the review’s sentence, nothing after it`,
    ).toBe(c.intro)
    expect(
      S[c.introKey],
      `${c.introKey}: one sentence, so the card shows it as one line (D4.31: the first line was 594 characters)`,
    ).not.toContain('. ')
    expect(
      S[c.aboutKey],
      `${c.aboutKey}: the rest of the old string, word for word with its \\n breaks, for the card’s About panel`,
    ).toBe(c.about)
  }
})

test('22 L2 D4.31 (its D1 review): each card shows its intro sentence and nothing of its about text at load, the panel closed', async () => {
  const main = await cardsPage()
  for (const c of INTRO_CARDS) {
    const box = card(main, c.title)
    expect(
      within(box).getAllByText(c.intro).length,
      `${c.title}: the card shows the whole intro key, once`,
    ).toBe(1)
    expect(
      isHidden(within(box).getByText(c.intro)),
      `${c.title}: the intro line is shown`,
    ).toBe(false)
    const toggle = introToggle(box, c.title)
    expect(
      paraTexts(introPanel(box, toggle)),
      `${c.title}: the panel holds the -about key, one paragraph per line, and nothing of the old string is left on the card`,
    ).toEqual(c.about.split('\n'))
    expect(
      isHidden(introPanel(box, toggle)),
      `${c.title}: the panel is closed at load, so its text is not shown`,
    ).toBe(true)
    expect(
      toggle.getAttribute('aria-expanded'),
      `${c.title}: the toggle says closed`,
    ).toBe('false')
  }
})

test('22 L2 D4.31 (its D1 review): opening a card’s About fills its panel with the -about key, one paragraph per line, the intro line staying shown', async () => {
  const main = await cardsPage()
  for (const c of INTRO_CARDS) {
    const box = card(main, c.title)
    const toggle = introToggle(box, c.title)
    fireEvent.click(toggle)
    expect(
      toggle.getAttribute('aria-expanded'),
      `${c.title}: the toggle says open`,
    ).toBe('true')
    const panel = introPanel(box, toggle)
    expect(isHidden(panel), `${c.title}: the panel is shown`).toBe(false)
    expect(
      paraTexts(panel),
      `${c.title}: the panel is the -about key, one paragraph per line`,
    ).toEqual(c.about.split('\n'))
    expect(
      isHidden(within(box).getByText(c.intro)),
      `${c.title}: the intro stays on the card`,
    ).toBe(false)
    fireEvent.click(toggle)
    expect(
      isHidden(introPanel(box, toggle)),
      `${c.title}: and closing it hides the panel again`,
    ).toBe(true)
  }
})

// ── The D1 review's advice 2: the toggle beside the card's h3 ─────────────

test('22 L2 D4.31 review A2: the card’s About toggle stands in the heading row beside its h3, not under the intro', async () => {
  const main = await cardsPage()
  for (const c of INTRO_CARDS) {
    const box = card(main, c.title)
    const h3 = cardH3(main, c.title)
    const toggle = introToggle(box, c.title)
    expect(
      toggle.closest('h3'),
      `${c.title}: the toggle is not inside the heading (a heading holds text)`,
    ).toBeNull()
    expect(
      toggle.parentElement,
      `${c.title}: the toggle and the h3 stand in one row`,
    ).toBe(h3.parentElement)
    // The row is an element of its own, not the card: a card lays its children
    // in a column (index.css `.card`), so "beside the title" needs a row.
    expect(
      h3.parentElement,
      `${c.title}: the heading row is its own box, not the card (a card is a column)`,
    ).not.toBe(box)
    expect(
      h3.nextElementSibling,
      `${c.title}: the toggle is the heading’s own neighbour, as a field's About follows its label`,
    ).toBe(toggle)
    expect(
      before(toggle, within(box).getByText(c.intro)),
      `${c.title}: the toggle sits above the intro, not under it`,
    ).toBe(true)
  }
})

test('22 L2 D4.31 review A2: the toggle stays the Tab stop right after the card’s h3, so it is the card’s first stop', async () => {
  const main = await cardsPage()
  const tabs = stops(main)
  for (const c of INTRO_CARDS) {
    const box = card(main, c.title)
    const toggle = introToggle(box, c.title)
    // The heading is the jump target and carries tabindex -1 (2h.196), so the
    // stop "right after the h3" is the card's first stop.
    expect(
      cardH3(main, c.title).getAttribute('tabindex'),
      `${c.title}: the heading is reached by the jump list, out of the Tab path`,
    ).toBe('-1')
    expect(
      tabs.find((el) => box.contains(el)),
      `${c.title}: the card's first Tab stop is its own About`,
    ).toBe(toggle)
  }
})

// ── The second review of the round (2026-11-27), D3: the look of the toggle ─

test('22 L2 D4.31 review A2: the card’s About stands --space-2 after its title and keeps the field About’s pill, outlined (the second review, D3)', async () => {
  const main = await cardsPage()
  const box = card(main, GPU)
  const h3 = cardH3(main, GPU)
  const toggle = introToggle(box, GPU)
  // The row the review measures in: the title's right edge to the toggle's left
  // is --space-2 (8 px), the way a field's About follows its label.
  const row = toggle.parentElement!
  expect(
    row,
    'the toggle and the h3 stand in the heading row (review A2)',
  ).toBe(h3.parentElement)
  // The 8 px is the row's gap, so no rule stretches the title (jsdom lays
  // nothing out: the sheet is what is pinnable, the measure is the UX check's).
  expect(
    reaching(row)
      .filter((r) => prop(r.body, 'gap') !== '')
      .map((r) => `${r.sel}: gap: ${norm(prop(r.body, 'gap'))}`),
    'one rule spaces the title and its About, at --space-2, as a field row does',
  ).toEqual(['.card-heading: gap: var(--space-2)'])
  // Nothing pushes the toggle to the row's end. The cause the review names is
  // `.card-heading > h3 { flex: 1 }` (index.css 1549-1552), which takes the
  // title the row's free space: the fix is to delete that declaration, and the
  // review leaves the rule's `margin: 0` alone, so the pin is of the
  // declarations, not of the rule.
  for (const name of [
    'flex',
    'flex-grow',
    'width',
    'margin-inline-start',
    'margin-left',
  ])
    expect(
      declaring(h3, name),
      `no rule reaches the card's title and declares ${name}: a stretched title pushes its own About to the end of the row, 467 px away at 1280 px (D3)`,
    ).toEqual([])
  expect(
    declaring(toggle, 'margin-inline-start'),
    'and no margin pushes the toggle itself to the end of the row',
  ).toEqual([])
  // The pill: the review's other cause is `.card-heading .about { padding-inline:
  // 0; border-color: transparent }` (index.css 1558-1561), which draws the
  // toggle as a bare text verb. The reference is the field's own About of the
  // same card ("GPU split mode"), which the review names as the look to keep,
  // so the card's toggle is pinned against what that one ends with, the padding
  // and the border read out of `.about`'s shorthand.
  const field = within(box).getByRole('button', {
    name: `${en('field-about')} ${en('model-profile-split-mode')}`,
  })
  const padding = valueOf(toggle, 'padding-inline')
  expect(
    padding,
    `the card's About keeps the inline padding of a field's About (declaring: ${[
      ...declaring(toggle, 'padding-inline'),
      ...declaring(toggle, 'padding'),
    ].join(' | ')}): D3 takes the rule away`,
  ).toBe(valueOf(field, 'padding-inline'))
  expect(
    padding,
    "and it is not zero: the toggle is a box with padding, as a field's About is",
  ).not.toBe('0')
  const border = valueOf(toggle, 'border-color')
  expect(
    border,
    `the card's About keeps the border of a field's About (declaring: ${[
      ...declaring(toggle, 'border-color'),
      ...declaring(toggle, 'border'),
    ].join(' | ')}): D3 takes the transparent border away`,
  ).toBe(valueOf(field, 'border-color'))
  expect(
    ['transparent', 'none', ''],
    "and the pill is drawn, not cleared: the toggle is outlined like a field's About",
  ).not.toContain(border)
})

// ── Item 7: 22 D4.32, the page head ───────────────────────────────────────

test('22 L2 D4.32: the model profile page settings heading is visually hidden, and a reader still finds it by role', async () => {
  const main = await cardsPage()
  const title = en('model-profile-settings-title')
  const h2 = within(main).getByRole('heading', { level: 2, name: title })
  expect(
    h2.className,
    'the eye sees no heading over the bar (D4.32)',
  ).toContain('visually-hidden')
  expect(h2.textContent, 'the word is the same key, unchanged').toBe(title)
  // The heading is the settings section's own, where the Save bar sits: the
  // section that holds the bar is the one the heading opens.
  expect(
    within(h2.parentElement!).getByRole('button', {
      name: en('model-profile-delete'),
    }),
    'the heading stands over the bar',
  ).toBeDefined()
  // The cards keep their own headings, which the jump list reaches: D4.32
  // hides the settings heading alone.
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .some((h) => !h.className.includes('visually-hidden')),
    'the cards keep visible headings',
  ).toBe(true)
})
