// Hearth H11a, the summary line under the model profile's h1 and the Assets
// card collapsed behind a disclosure in its heading
// (fe/briefs/08-hearth-h10-h11.md, section 2): the DOM half. H11a.1 to H11a.8,
// H11a.10 and H11a.11 are pinned here on the rendered page; H11a.9 is lists of
// declarations, pinned at the bottom of this file as text of index.css the way
// HearthH10.test.ts reads it (comments dropped, rules from the brace balance,
// specificity from the selector), and the reader is a local copy because a test
// file imports no other test file.
//
// Enter and Space (H11a.5) are what a native button gives: fireEvent fires no
// key activation in jsdom, so `type="button"` is the proxy the test can reach,
// as in H10.3, and the two keys go to the designer's UX pass. The chevron's
// turn, the ring around the toggle and the summary wrapping a long file name at
// 320 px are for the same pass (section 5).
//
// The card's own content, the Assets rows, is 2d.73 to 2d.76's in
// ModelProfileContent.test.tsx: it must not change (section 6) beyond the
// Engine row H11b.1 puts first, so it is read here only to show that the
// disclosure moved it behind `hidden` and left the description list as it was.
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
import { t } from './strings.ts'
import {
  before,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
} from './test/bff.ts'
import { cardHeadings } from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const ASSETS = en('model-profile-assets')
/** H11a.3: the three ids the slice gives the heading, the toggle and the body. */
const HEADING_ID = 'model-profile-card-assets'
const TOGGLE_ID = 'model-profile-assets-toggle'
const BODY_ID = 'model-profile-assets-body'
/** H11a.3: the chevron the disclosure carries (Icon.tsx, H8a's shape). */
const CHEVRON = 'm6 9 6 6 6-6'
/** The Assets rows, in the GUI's order (2d.73). */
const LABELS = [
  en('model-profile-model-file'),
  en('model-profile-project-url'),
  // Not a string: an acronym, as the GUI labels it.
  'MMProj',
  en('model-profile-draft-model'),
  en('model-profile-drafter'),
  en('model-profile-ngram-drafters'),
]
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'
const DRAFT = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0-draft.gguf'
const MMPROJ = '/var/lib/private-ai/.llama.cpp/models/mmproj-F16.gguf'
const URL = 'https://huggingface.co/org/repo'
const ENGINE = 'llama.cpp'
/** H11a.9: the selectors of the two things the slice styles. */
const SUMMARY_SEL = '.model-profile-summary'
const TOGGLE_SEL = '.assets-card h3 button'
const CHEVRON_SEL = ".assets-card h3 button[aria-expanded='true'] .icon"
/** The card headings in the page order of 83: Assets, then the nine of 2e. */
const CARD_TITLES = cardHeadings()
/** The nine 2e cards: the jump list's entries after Assets. */
const OTHER_TITLES = CARD_TITLES.slice(1)
/** A path case whose file name is the no-model line. */
const NO_FILE = '(no model file)'

let scrolled: Element[] = []

beforeEach(() => {
  startAt(HASH)
  scrolled = []
  // jsdom has no scrollIntoView; a jump calls it on the heading.
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this)
  }
})

afterEach(() => {
  cleanup()
  startAt('')
})

/** The read every test starts from: every Assets row filled, so a row that
 * shows `(none)` is a miss and not a fixture. `extra` replaces top-level
 * members of the document, `values` the keys of the section (the page reads
 * the summary line from `values['model']`, so a path case lands there). */
function assetsDoc(
  extra: Record<string, unknown> = {},
  values: Record<string, string> = {},
) {
  return modelProfileDoc(
    {
      model: MODEL,
      mmproj: MMPROJ,
      'model-draft': DRAFT,
      'spec-type': 'ngram-mod,draft-mtp',
      'ctx-size': '172032',
      ...values,
    },
    { revision: 'r1', projectUrl: URL, ...extra },
  )
}

/** A read of the model profile, awaited to the h1 and the Assets heading. */
async function form(doc = assetsDoc()) {
  const bff = fakeBff({ modelProfile: () => reply(200, doc) })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: ASSETS })
  return { bff, main }
}

/** The element H11a.3 gives the card, by its id, so a miss names the id. */
function byId(id: string, what: string): HTMLElement {
  const el = document.getElementById(id)
  expect(el, `${what}: #${id}`).not.toBeNull()
  return el as HTMLElement
}

const toggleOf = (): HTMLElement => byId(TOGGLE_ID, 'the Assets disclosure')
const bodyOf = (): HTMLElement => byId(BODY_ID, 'the Assets body')
const headingOf = (): HTMLElement => byId(HEADING_ID, "the Assets card's h3")

const textOf = (el: HTMLElement): string =>
  (el.textContent ?? '').replace(/\s+/g, ' ').trim()

/** The card's state, as the two attributes H11a.3 pins: the toggle's expanded
 * state, and whether the body carries `hidden`. An element that is not there
 * reads `[null, null]`: the H11a.6 spies record the state while the page is
 * being built, and an expectation then says what is missing. */
const state = (): [string | null, boolean | null] => {
  const toggle = document.getElementById(TOGGLE_ID)
  const body = document.getElementById(BODY_ID)
  return [
    toggle === null ? null : toggle.getAttribute('aria-expanded'),
    body === null ? null : body.hasAttribute('hidden'),
  ]
}

const openState: [string, false] = ['true', false]
const closedState: ['false', true] = ['false', true]

function expectOpen(what: string): void {
  expect(state(), `${what}: open`).toEqual(openState)
  expect(
    isHidden(document.getElementById(BODY_ID)),
    `${what}: the body is shown`,
  ).toBe(false)
}

function expectClosed(what: string): void {
  expect(state(), `${what}: closed`).toEqual(closedState)
  expect(
    isHidden(document.getElementById(BODY_ID)),
    `${what}: the body is not shown`,
  ).toBe(true)
}

/** The card's `section`, from its h3 (Card.tsx). */
function cardOf(main: HTMLElement): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: ASSETS })
  const card = h3.closest('section')
  expect(card, `${ASSETS}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The jump list's entries, in the list's order (H10.1: the nav holds the ul). */
function entries(main: HTMLElement): HTMLElement[] {
  const list = main.querySelector<HTMLElement>('#model-profile-jump-list')
  expect(list, 'the jump list of 196').not.toBeNull()
  return within(list as HTMLElement).getAllByRole('button')
}

/** The elements Tab reaches inside `root`, in DOM order (91, 196). */
function tabbableElements(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** The pairs of the card's description list, as text, each label with the
 * value that follows it. The list sits behind `hidden` when the card is
 * closed, which closes the role queries to it, so this reads the DOM. */
function pairs(card: HTMLElement): [string, string][] {
  const dls = card.querySelectorAll('dl')
  expect(dls.length, 'one description list').toBe(1)
  const out: [string, string][] = []
  for (const dt of dls[0]!.querySelectorAll('dt')) {
    const dd = dt.nextElementSibling
    expect(dd, `${textOf(dt)}: its value follows`).not.toBeNull()
    out.push([textOf(dt), textOf(dd as HTMLElement)])
  }
  return out
}

/** The summary line, by the class H11a.1 gives it. */
function summaryOf(main: HTMLElement): HTMLElement {
  const p = main.querySelector('p.model-profile-summary')
  expect(p, 'the summary line: p.model-profile-summary').not.toBeNull()
  return p as HTMLElement
}

/** A save the manager refuses with `body`, after one edit of a settings field.
 * `openCard` opens the Assets card first, so the refusal is seen with the card
 * in that state. */
async function refused(
  body: Record<string, unknown>,
  openCard: boolean,
): Promise<HTMLElement> {
  const bff = fakeBff({
    modelProfile: () => reply(200, assetsDoc()),
    modelProfileSave: () => reply(422, body),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: ASSETS })
  if (openCard) fireEvent.click(toggleOf())
  const ctx = document.getElementById('model-profile-ctx-size-control')
  expect(ctx, 'the Context size field').not.toBeNull()
  fireEvent.change(ctx as HTMLElement, { target: { value: '65536' } })
  const save = within(main).getByRole('button', { name: en('save-button') })
  save.focus()
  fireEvent.click(save)
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  return main
}

// ── H11a.1: the summary line ─────────────────────────────────────────────

const PATHS: [string, Record<string, string> | null, string][] = [
  ['a path with /', { model: MODEL }, 'Qwen3-27B-Q8_0.gguf'],
  [
    'a path with \\',
    { model: '\\\\server\\share\\models\\Qwen3-27B-Q8_0.gguf' },
    'Qwen3-27B-Q8_0.gguf',
  ],
  [
    'a path that mixes / and \\',
    { model: '/var/lib/private-ai\\models\\Qwen3-27B-Q8_0.gguf' },
    'Qwen3-27B-Q8_0.gguf',
  ],
  [
    'a path with an empty segment',
    { model: '/var/lib/models//Qwen3-27B-Q8_0.gguf' },
    'Qwen3-27B-Q8_0.gguf',
  ],
  // H11a.1: `file` is the LAST NON-EMPTY segment, so a path ending in a
  // separator names its last directory; the no-model line is only for an
  // absent key, an empty value, or a value with no non-empty segment.
  ['a path ending in \\', { model: '\\\\server\\share\\' }, 'share'],
  ['the key absent', null, NO_FILE],
  ['the value empty', { model: '' }, NO_FILE],
  ['a path ending in /', { model: '/var/lib/models/' }, 'models'],
]

test.each(PATHS)(
  'H11a.1: the line under the h1 names the engine of the read and the model file, %s',
  async (what, model, file) => {
    // The key absent: a `values` with no `model` at all, as a read that carries
    // no such key answers.
    const doc =
      model === null
        ? assetsDoc({ values: { 'ctx-size': '172032', 'spec-type': 'none' } })
        : assetsDoc({}, model)
    const { main } = await form(doc)
    const line = summaryOf(main)
    const expectText =
      file === NO_FILE
        ? en('model-profile-summary-no-model', ENGINE)
        : en('model-profile-summary', ENGINE, file)
    expect(textOf(line), `H11a.1: ${what}`).toBe(expectText)
  },
)

test('H11a.1: the line is the first thing under the h1, before the state line, a plain p that is no heading and no live region', async () => {
  // A switched-off profile: the state line of 24 A2.1 (the page's own switch
  // and the short word of its state) is the element the line goes before, so
  // the order of the page top is pinned in one read.
  const { main } = await form(assetsDoc({ enabled: false }))
  const h1 = within(main).getByRole('heading', { level: 1, name: ID })
  const line = summaryOf(main)
  expect(h1.nextElementSibling, 'H11a.1: right after the h1').toBe(line)
  const off = within(main).getByText(en('model-profile-off-short'))
  expect(before(line, off), 'H11a.1: before the state line').toBe(true)
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  expect(before(off, h2), 'H11a.1: the line is above the h2').toBe(true)
  expect(line.tagName, 'H11a.1: a p').toBe('P')
  expect(
    within(main).queryAllByRole('heading', { name: textOf(line) }),
    'H11a.1: not a heading',
  ).toEqual([])
  expect(line.getAttribute('role'), 'H11a.1: no role').toBeNull()
  expect(line.getAttribute('aria-live'), 'H11a.1: no live region').toBeNull()
  expect(
    within(main)
      .queryAllByRole('status')
      .filter((s) => s === line).length,
    'H11a.1: no status: the save bar keeps the only one (135)',
  ).toBe(0)
})

// ── H11a.2: the two strings ──────────────────────────────────────────────

test('H11a.2: the two new keys are the brief English, in the test table and in the fe strings file the page reads', () => {
  expect(en('model-profile-summary', ENGINE, 'Qwen.gguf'), 'the key').toBe(
    'Engine llama.cpp, model Qwen.gguf',
  )
  expect(
    en('model-profile-summary-no-model', ENGINE),
    'the key with no file',
  ).toBe('Engine llama.cpp, no model file')
  expect(
    t('model-profile-summary', ENGINE, 'Qwen.gguf'),
    'H11a.2: in the strings file',
  ).toBe('Engine llama.cpp, model Qwen.gguf')
  expect(
    t('model-profile-summary-no-model', ENGINE),
    'H11a.2: in the strings file',
  ).toBe('Engine llama.cpp, no model file')
})

// ── H11a.3: the card, the toggle, the body ───────────────────────────────

test('H11a.3: the Assets card is a section.card.assets-card: its h3 holds the toggle and nothing else, then the intro, then a body div holding today dl, hidden', async () => {
  const { main } = await form()
  const card = cardOf(main)
  expect(Array.from(card.classList).sort(), 'H11a.3: the card classes').toEqual(
    ['assets-card', 'card'],
  )
  const h3 = headingOf()
  expect(h3.id, 'H11a.3: the jump target keeps its id').toBe(HEADING_ID)
  expect(h3.getAttribute('tabindex'), 'H11a.3: out of the Tab path').toBe('-1')
  expect(h3.closest('section'), 'H11a.3: the heading is in the card').toBe(card)

  expect(h3.children.length, 'H11a.3: the h3 holds only the toggle').toBe(1)
  const toggle = h3.children[0] as HTMLElement
  expect(toggle, 'H11a.3: the h3s child is the toggle').toBe(toggleOf())
  expect(toggle.tagName, 'section 0: a native button').toBe('BUTTON')
  expect(
    toggle.getAttribute('type'),
    'section 0: a type=button gives Enter and Space',
  ).toBe('button')
  expect(
    toggle.getAttribute('aria-controls'),
    'H11a.3: it controls the body',
  ).toBe(BODY_ID)
  expect(textOf(toggle), 'H11a.8: the text it carries').toBe(ASSETS)
  const icon = toggle.querySelector('.icon')
  expect(icon, 'H11a.3: the chevron beside the text').not.toBeNull()
  expect(
    icon!.getAttribute('aria-hidden'),
    'an Icon is decoration: hidden from the accessibility tree',
  ).toBe('true')
  expect(
    icon!.querySelector('path')?.getAttribute('d'),
    'the chevron-down shape of H8a',
  ).toBe(CHEVRON)
  expect(
    Array.from(toggle.children).indexOf(icon as unknown as Element),
    'section 0: the chevron comes after the text',
  ).toBe(toggle.children.length - 1)

  // The card's three parts, in order: the heading, the read-only note (always
  // shown), the body.
  const intro = card.querySelector('p.intro')
  expect(intro, 'H11a.3: the read-only note').not.toBeNull()
  expect(textOf(intro as HTMLElement), 'H11a.3: its text (195)').toBe(
    en('model-profile-assets-read-only'),
  )
  expect(
    isHidden(intro as HTMLElement),
    'H11a.3: always shown: the closed card leaves the note out, not the note in',
  ).toBe(false)
  const body = bodyOf()
  expect(Array.from(card.children), 'H11a.3: h3, intro, body').toEqual([
    h3,
    intro,
    body,
  ])
  expect(body.closest('section'), 'H11a.3: the body is in the card').toBe(card)

  // Today's description list, unchanged, inside it.
  const dl = body.querySelector('dl.assets')
  expect(
    dl,
    'H11a.3: the body holds the dl of 2d.73, class and all',
  ).not.toBeNull()
  expect(dl!.parentElement, 'H11a.3: the dl sits in the body').toBe(body)
  expect(
    pairs(card),
    'H11a.3, H11b.1: the Engine row leads, the six of 2d.73 follow unchanged',
  ).toEqual([
    [en('model-profile-engine'), ENGINE],
    [LABELS[0], MODEL],
    [LABELS[1], URL],
    ['MMProj', MMPROJ],
    [LABELS[3], DRAFT],
    [LABELS[4], 'draft-mtp'],
    [LABELS[5], 'ngram-mod'],
  ])
})

test('H11a.4: the card mounts closed, opens on a click, and mounts closed again: a view state is never remembered', async () => {
  const first = await form()
  expectClosed('H11a.4: closed on mount')
  fireEvent.click(toggleOf())
  expectOpen('H11a.4: the click opened it')
  // A second mount of the page: the state of the first one is not kept
  // (section 0: no storage for a view state).
  cleanup()
  startAt(HASH)
  await form()
  expectClosed('H11a.4: closed on the second mount')
  expect(first.main.isConnected, 'the first page is gone').toBe(false)
})

// ── H11a.5: the toggle ───────────────────────────────────────────────────

test('H11a.5: a click opens the card and a click closes it, the expanded state and hidden flip together, and the focus stays on the toggle', async () => {
  const { main } = await form()
  const toggle = toggleOf()
  toggle.focus()
  fireEvent.click(toggle)
  expectOpen('H11a.5: opened')
  expect(document.activeElement, 'H11a.5: the focus stays on the toggle').toBe(
    toggle,
  )
  fireEvent.click(toggle)
  expectClosed('H11a.5: closed again')
  expect(
    document.activeElement,
    'H11a.5: closing keeps the focus where the user left it',
  ).toBe(toggle)
  // H11a.5: the body holds nothing focusable, so closing strands nothing
  // (2d.73). The card holds one tab stop, the toggle itself.
  expect(tabbableElements(bodyOf()), 'H11a.5: no control in the body').toEqual(
    [],
  )
  expect(tabbableElements(cardOf(main)), 'the card: one tab stop').toEqual([
    toggle,
  ])
  // Enter and Space are what the native button gives (section 0, WAI-ARIA APG
  // Disclosure): fireEvent fires no key activation in jsdom, so the type is the
  // proxy the test can reach, and the two keys go to the UX pass.
  expect(
    toggle.getAttribute('type'),
    'H11a.5: a type=button gives Enter and Space (reported for the UX pass)',
  ).toBe('button')
})

// ── H11a.6: a jump ───────────────────────────────────────────────────────

test('H11a.6: the Assets jump opens the card and moves the focus and the scroll to its h3, the open state in the DOM before both (the order of the H10 review)', async () => {
  const { main } = await form()
  const h3 = headingOf()
  // The state of the card in the DOM at the moment of each call, the way the
  // review of section 7 measured it: an open that lands after the focus moves
  // leaves the heading where the closed card put it.
  const focusSaw: [string | null, boolean | null][] = []
  const scrollSaw: [string | null, boolean | null][] = []
  h3.focus = () => {
    focusSaw.push(state())
    HTMLElement.prototype.focus.call(h3)
  }
  h3.scrollIntoView = () => {
    scrollSaw.push(state())
    Element.prototype.scrollIntoView.call(h3)
  }
  fireEvent.click(entries(main)[0]!)
  // Both spies ran, so the records below are a jump and not an empty list.
  expect(focusSaw, 'the jump moved the focus to the h3').toHaveLength(1)
  expect(scrollSaw, 'the jump scrolled the h3 into view').toHaveLength(1)
  expect(focusSaw[0], 'the card was open when the focus moved').toEqual(
    openState,
  )
  expect(scrollSaw[0], 'and when the scroll ran').toEqual(openState)
  expect(document.activeElement, 'H11a.6: focus on the h3').toBe(h3)
  expect(scrolled, 'H11a.6: the h3 scrolled into view').toContain(h3)
  expectOpen('H11a.6: the jump left the card open')
  expect(window.location.hash, 'a jump writes no hash (196)').toBe(HASH)
})

test('H11a.6: a card already open stays open under the Assets jump, and the nine other jumps leave Assets as it is and still land on their own h3', async () => {
  const { main } = await form()
  // The entry of a card, by its text (H10.1: the jump list is buttons in the
  // list's order).
  const jumpTo = (title: string): HTMLElement => {
    const b = entries(main).find((e) => textOf(e) === title)
    expect(b, `the jump entry of ${title}`).not.toBeUndefined()
    return b as HTMLElement
  }
  const assets = jumpTo(ASSETS)
  expect(
    entries(main).indexOf(assets),
    'the list opens with Assets (196)',
  ).toBe(0)

  // Open, then a jump to Assets: nothing closes.
  fireEvent.click(toggleOf())
  expectOpen('opened before the jump')
  fireEvent.click(assets)
  expectOpen('H11a.6: an open card stays open')

  // Closed, then each of the nine 2e jumps: Assets stays closed, and the jump
  // itself still works (section 6: the nine jumps unchanged).
  fireEvent.click(toggleOf())
  expectClosed('closed before the nine jumps')
  for (const title of OTHER_TITLES) {
    fireEvent.click(jumpTo(title))
    const h3 = within(main).getByRole('heading', { level: 3, name: title })
    expect(document.activeElement, `${title}: focus on its h3`).toBe(h3)
    expect(state(), `${title}: Assets left as it was`).toEqual(closedState)
  }
})

// ── H11a.7: a save that is refused ───────────────────────────────────────

/** The 143 refusal: the manager refuses the whole profile for no model file,
 * naming the Assets key `model`. */
const ASSET_REFUSAL = {
  key: 'model-profile-no-model',
  args: [ID],
  field: 'model',
}

test.each([
  { state: 'open', openCard: true },
  { state: 'closed', openCard: false },
])(
  'H11a.7: a 422 naming an Assets key leaves the card $state, tells the bar, keeps the focus on Save changes and marks no field',
  async ({ openCard }) => {
    const main = await refused(ASSET_REFUSAL, openCard)
    const text = en('api-model-profile-no-model', ID)
    const alert = (
      await within(main).findByText(en('save-refused', text))
    ).closest('[role="alert"]')
    expect(alert, '143: the refusal is in role="alert"').not.toBeNull()
    expect(
      [...main.querySelectorAll('[aria-invalid="true"]')],
      '143: no field is marked (the key names an Assets row)',
    ).toEqual([])
    expect(document.activeElement, '143: the focus stays on Save changes').toBe(
      within(main).getByRole('button', { name: en('save-button') }),
    )
    expect(state(), 'H11a.7: the save left the card as it was').toEqual(
      openCard ? openState : closedState,
    )
    expect(
      textOf(summaryOf(main)),
      'H11a.7: the summary line says the same as before the save',
    ).toBe(en('model-profile-summary', ENGINE, 'Qwen3-27B-Q8_0.gguf'))
  },
)

// ── H11a.8 and H11a.10: the names and the reading ────────────────────────

test('H11a.8: the h3, the toggle and the jump entry are all named Assets, and the card holds exactly one button', async () => {
  const { main } = await form()
  const card = cardOf(main)
  expect(textOf(headingOf()), 'H11a.8: the heading reads Assets').toBe(ASSETS)
  expect(
    within(main).getAllByRole('button', { name: ASSETS }),
    'H11a.8: two controls carry the name (2.5.3, 4.1.2)',
  ).toEqual([
    // The jump list is above the cards in the DOM: the entry, then the toggle.
    entries(main)[0]!,
    toggleOf(),
  ])
  expect(within(card).getAllByRole('button'), 'the card: one button').toEqual([
    toggleOf(),
  ])
})

test('H11a.10: the reading is the heading, then the button collapsed, then the read-only note', async () => {
  const { main } = await form()
  const h3 = within(main).getByRole('heading', { level: 3, name: ASSETS })
  expect(
    within(main).getByRole('button', { name: ASSETS, expanded: false }),
    'the collapsed button is the disclosure',
  ).toBe(toggleOf())
  expect(
    within(main).queryByRole('button', { name: ASSETS, expanded: true }),
    'nothing says open on arrival',
  ).toBeNull()
  // What follows the heading is the note: the reading goes heading, button,
  // note.
  const intro = h3.parentElement!.querySelector('p.intro')
  expect(intro, 'the read-only note (195)').not.toBeNull()
  expect(textOf(intro as HTMLElement), 'its text').toBe(
    en('model-profile-assets-read-only'),
  )
  expect(
    before(h3, intro as HTMLElement),
    'H11a.10: the note comes after the heading and its button',
  ).toBe(true)
  expect(
    isHidden(bodyOf()),
    'H11a.10: closed, the reading ends at the note: the rows are behind hidden',
  ).toBe(true)
})

// ── H11a.11: nothing else moves ──────────────────────────────────────────

test('H11a.11: the notice, the Engine row of Assets (H11b), the h2, the nine cards of 83 in their order, the ten jumps and the save bar stay as they are', async () => {
  const { main } = await form(assetsDoc({ engineAvailable: false }))
  const notice = main.querySelector('p.notice')
  expect(notice, 'H11a.11: the engine Notice of 72').not.toBeNull()
  expect(
    textOf(notice as HTMLElement),
    'H11a.11: it names the engine of the read',
  ).toContain(ENGINE)
  expect(
    within(main).getByRole('heading', {
      level: 2,
      name: en('model-profile-settings-title'),
    }),
    'H11a.11: the h2',
  ).toBeTruthy()
  expect(
    pairs(cardOf(main))[0],
    'H11a.11, H11b.1: the Engine row of Assets says the engine of the read',
  ).toEqual([
    en('model-profile-engine'),
    en('model-profiles-engine-not-available', ENGINE),
  ])
  expect(
    [...main.querySelectorAll<HTMLElement>('section.card > h3')].map((h) =>
      textOf(h),
    ),
    'H11a.11: the ten cards in the order of 83',
  ).toEqual(CARD_TITLES)
  expect(
    entries(main).map((b) => textOf(b)),
    'H11a.11: the ten jumps of 196, in card order',
  ).toEqual(CARD_TITLES)
  expect(main.querySelector('.save-bar'), 'H11a.11: the save bar').toBeTruthy()
  expect(
    within(main).getByRole('button', { name: en('save-button') }),
    'H11a.11: Save changes',
  ).toBeTruthy()
})

// ── H11a.9: the look, as text of index.css ───────────────────────────────
// Comments dropped, the rules taken from the brace balance, the specificity
// from the selector: the reader of HearthH10.test.ts, copied here, because a
// test file imports no other test file.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  flat: string
  body: string
  at: number
  depth: number
}

const rulesOf = (css: string): Rule[] => {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, (m) => ' '.repeat(m.length))
  const opened = (upto: number): number =>
    (stripped.slice(0, upto).match(/\{/g) ?? []).length -
    (stripped.slice(0, upto).match(/\}/g) ?? []).length
  return [...stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    sel: m[1].trim(),
    flat: m[1].trim().replace(/\s+/g, ' '),
    body: m[2],
    at: m.index ?? 0,
    depth: opened(m.index ?? 0),
  }))
}

const RULES: Rule[] = rulesOf(CSS)

/** The value of a declaration; the last one when a block repeats the property.
 * Runs of whitespace collapse to one space, so Prettier re-wrapping a long
 * value never reds on the wrap. */
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .trim()
        .replace(/\s+/g, ' ')
  }
  return val
}

/** A throw names the miss, so "the rule is missing" never reads as a typo. */
const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length ? ` (${list.map((r) => r.flat).join(' | ')})` : ''),
    )
  return list[0]
}

/** The items of a selector list, split on the commas outside brackets. */
const listItems = (sel: string): string[] => {
  const items: string[] = []
  let cur = ''
  let depth = 0
  for (const ch of sel) {
    if (ch === '(' || ch === '[') depth++
    else if (ch === ')' || ch === ']') depth--
    if (ch === ',' && depth === 0) {
      items.push(cur)
      cur = ''
    } else cur += ch
  }
  items.push(cur)
  return items.map((s) => s.replace(/\s+/g, ' ').trim()).filter((s) => s !== '')
}

const topLevel = (): Rule[] => RULES.filter((r) => r.depth === 0)

/** The rule whose selector list is exactly this selector. */
const bySel = (sel: string, what: string): Rule =>
  single(
    topLevel().filter(
      (r) => listItems(r.sel).length === 1 && listItems(r.sel)[0] === sel,
    ),
    `the rule "${sel}" (${what})`,
  )

/** A specificity in three parts: ids, then classes, attributes and
 * pseudo-classes, then elements and pseudo-elements. */
type Spec = [number, number, number]

const TOKEN =
  /\[[^\]]*\]|::[a-zA-Z-]+|:[a-zA-Z-]+(?:\([^)]*\))?|\.[a-zA-Z-]+|#[a-zA-Z-]+|[a-zA-Z][a-zA-Z0-9-]*|[*+>~]|\s+/g

const specOf = (raw: string): Spec => {
  const s: Spec = [0, 0, 0]
  for (const m of raw.trim().matchAll(TOKEN)) {
    const t = m[0]
    if (/^\s/.test(t) || t === '*' || t === '+' || t === '>' || t === '~')
      continue
    if (t[0] === '#') s[0]++
    else if (t.startsWith('::')) s[2]++
    else if (t[0] === '.' || t[0] === '[' || t[0] === ':') s[1]++
    else s[2]++
  }
  return s
}

const specGt = (a: Spec, b: Spec): boolean =>
  a[0] !== b[0] ? a[0] > b[0] : a[1] !== b[1] ? a[1] > b[1] : a[2] > b[2]

const specEq = (a: Spec, b: Spec): boolean =>
  a[0] === b[0] && a[1] === b[1] && a[2] === b[2]

/** The elements the slice styles, as the page renders them, from the element
 * outwards. */
type El = { tag: string; id: string; classes: string[] }

const MAIN: El = { tag: 'main', id: 'main', classes: [] }
const LAYOUT: El = { tag: 'div', id: '', classes: ['layout'] }
const ROOT: El = { tag: 'div', id: 'root', classes: [] }
const BODY_EL: El = { tag: 'body', id: '', classes: [] }
const HTML: El = { tag: 'html', id: '', classes: [] }
const SECTION: El = { tag: 'section', id: '', classes: [] }
const ASSETS_CARD: El = {
  tag: 'section',
  id: '',
  classes: ['card', 'assets-card'],
}

const CHAINS: Record<string, El[]> = {
  summary: [
    { tag: 'p', id: '', classes: ['model-profile-summary'] },
    MAIN,
    LAYOUT,
    ROOT,
    BODY_EL,
    HTML,
  ],
  toggle: [
    { tag: 'button', id: TOGGLE_ID, classes: [] },
    { tag: 'h3', id: HEADING_ID, classes: [] },
    ASSETS_CARD,
    SECTION,
    MAIN,
    LAYOUT,
    ROOT,
    BODY_EL,
    HTML,
  ],
  body: [
    { tag: 'div', id: BODY_ID, classes: [] },
    ASSETS_CARD,
    SECTION,
    MAIN,
    LAYOUT,
    ROOT,
    BODY_EL,
    HTML,
  ],
}

/** A selector part split into its compounds at the whitespace and the
 * combinators OUTSIDE brackets and parentheses. */
const chainOf = (raw: string): { comb: string; text: string }[] => {
  const out: { comb: string; text: string }[] = []
  let cur = ''
  let comb = ' '
  let depth = 0
  const push = (): void => {
    if (cur === '') return
    out.push({ comb, text: cur })
    cur = ''
    comb = ' '
  }
  for (const ch of raw.trim()) {
    if (ch === '(' || ch === '[') depth++
    else if (ch === ')' || ch === ']') depth--
    if (depth > 0 || !/[\s>+~]/.test(ch)) {
      cur += ch
      continue
    }
    if (/\s/.test(ch)) push()
    else {
      push()
      comb = ch
    }
  }
  push()
  return out
}

/** One compound: its tag, its id, its classes. An attribute selector, a
 * pseudo-class or a pseudo-element is a constraint the chain (tag, id, classes)
 * cannot carry, so a part that holds one is out of the scan: the sheet's
 * `[hidden] { display: none !important }` would else win every `display` read,
 * and `.card h3:focus-visible` every `outline` read. */
const compoundOf = (text: string): El => {
  const out = { tag: '', id: '', classes: [] as string[] }
  for (const m of text.matchAll(TOKEN)) {
    const t = m[0]
    if (t[0] === '#') out.id = t.slice(1)
    else if (t[0] === '.') out.classes.push(t.slice(1))
    else if (t[0] === '[' || t[0] === ':') continue
    else if (t !== '*') out.tag = t
  }
  return out
}

const fits = (c: El, el: El): boolean =>
  (c.tag === '' || c.tag === el.tag) &&
  (c.id === '' || c.id === el.id) &&
  c.classes.every((k) => el.classes.includes(k))

/** Does this selector part reach this chain's element? Its LAST compound fits
 * the element; the compounds before it are walked up the chain. */
const matchesChain = (els: El[], raw: string): boolean => {
  const chain = chainOf(raw)
  if (chain.length === 0) return false
  if (chain.some((c) => c.text.includes('[') || c.text.includes(':')))
    return false
  if (!fits(compoundOf(chain[chain.length - 1].text), els[0])) return false
  let i = 1
  for (let k = chain.length - 2; k >= 0; k--) {
    const c = compoundOf(chain[k].text)
    const comb = chain[k + 1].comb
    if (comb === '+' || comb === '~') return false
    if (comb === '>') {
      if (i >= els.length || !fits(c, els[i])) return false
      i++
      continue
    }
    while (i < els.length && !fits(c, els[i])) i++
    if (i >= els.length) return false
    i++
  }
  return true
}

/** The highest specificity among the parts of a selector list that reach the
 * chain; null when none does. */
const chainSpec = (sel: string, els: El[]): Spec | null => {
  let best: Spec | null = null
  for (const part of listItems(sel)) {
    if (!matchesChain(els, part)) continue
    const s = specOf(part)
    if (best === null || specGt(s, best)) best = s
  }
  return best
}

/** The rule with the highest claim on this chain and property: highest
 * specificity, and at a tie the one written last (the cascade's own order).
 * Only the top level is scanned: the slice styles no media window. */
const claim = (els: El[], p: string): { rule: Rule; spec: Spec } | null => {
  let best: { rule: Rule; spec: Spec } | null = null
  for (const r of topLevel()) {
    if (prop(r.body, p) === '') continue
    const s = chainSpec(r.sel, els)
    if (s === null) continue
    if (
      best === null ||
      specGt(s, best.spec) ||
      (specEq(s, best.spec) && r.at > best.rule.at)
    )
      best = { rule: r, spec: s }
  }
  return best
}

/** The look H11a.9 asks for, as property and value, and the selector that owns
 * each one. */
const LOOK: [string, string, [string, string][]][] = [
  [
    'summary',
    SUMMARY_SEL,
    [
      ['margin', '0 0 var(--space-4)'],
      ['color', 'var(--muted)'],
      ['overflow-wrap', 'anywhere'],
    ],
  ],
  [
    'toggle',
    TOGGLE_SEL,
    [
      ['display', 'flex'],
      ['width', '100%'],
      ['justify-content', 'space-between'],
      ['align-items', 'center'],
      ['min-height', '32px'],
      ['padding', 'var(--space-1) 0'],
      ['border', '0'],
      ['background', 'transparent'],
      ['font', 'inherit'],
      ['color', 'var(--text)'],
      ['text-align', 'left'],
    ],
  ],
]

test('H11a.9: .model-profile-summary carries the margin, the muted colour and the wrapping of a long file name', () => {
  const rule = bySel(SUMMARY_SEL, 'the summary line')
  for (const [p, val] of LOOK[0][2])
    expect(prop(rule.body, p), `${SUMMARY_SEL}: ${p}`).toBe(val)
})

test('H11a.9: .assets-card h3 button carries the eleven declarations of the brief', () => {
  const rule = bySel(TOGGLE_SEL, 'the Assets disclosure')
  for (const [p, val] of LOOK[1][2])
    expect(prop(rule.body, p), `${TOGGLE_SEL}: ${p}`).toBe(val)
})

test('H11a.9: the chevron turns with the state and nothing eases it', () => {
  const rule = bySel(CHEVRON_SEL, 'the chevron of an open Assets card')
  expect(prop(rule.body, 'rotate'), `${CHEVRON_SEL}: rotate`).toBe('180deg')
  expect(
    prop(rule.body, 'transition'),
    'H11a.9: no transition: the state is the moment, not an animation',
  ).toBe('')
})

test('H11a.9: the cascade: the summary keeps its three declarations over the p rule, and the toggle keeps every one of its look over the global button rule', () => {
  for (const [name, sel, look] of LOOK) {
    for (const [p, val] of look) {
      const won = claim(CHAINS[name]!, p)
      expect(won, `${p}: the rule that owns it`).not.toBeNull()
      expect(
        listItems(won!.rule.sel),
        `${p}: the highest claim on it (specificity, then source order)`,
      ).toEqual([sel])
      expect(prop(won!.rule.body, p), `${p}: its value`).toBe(val)
    }
  }
})

test('H11a.9: the Assets body is hidden by the global [hidden] rule and by no rule of the slice (section 0: it carries hidden, not a class)', () => {
  expect(
    claim(CHAINS['body']!, 'display'),
    'no rule of the sheet sets display on the body: hidden does',
  ).toBeNull()
  const hidden = single(
    topLevel().filter((r) => r.flat === '[hidden]'),
    'the global [hidden] rule',
  )
  expect(prop(hidden.body, 'display'), 'the global rule').toBe(
    'none !important',
  )
  expect(
    hidden.body.includes('!important'),
    'section 0: it wins over any display rule, which is why the Assets body uses hidden',
  ).toBe(true)
})

test('H11a.9: the ring rule of .card h3:focus-visible is the one of 130, unchanged (section 6)', () => {
  const rule = bySel(
    '.card h3:focus-visible',
    'the ring a jump lands with (130)',
  )
  expect(prop(rule.body, 'outline'), 'the ring').toBe(
    'var(--ring-w) solid var(--accent)',
  )
  expect(prop(rule.body, 'outline-offset'), 'its offset').toBe('2px')
})
