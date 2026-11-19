// The CONTENT of the model profile page #/profiles/model/<id>, read-only (fe/briefs/02-models.md,
// sections 2d.5 to 2d.9): the order below the h1, the line for a switched-off
// model profile, the Engine select of D2.2, the Assets card as a description list, the
// keyboard path and what a screen reader says, and the reply shape the fe checks.
// The route, the back link, the h1, the read and its states are ModelProfilePage.test.tsx
// (2d.1 to 2d.4), the list's tab path and table reading are ModelProfiles.test.tsx and
// ModelProfilesSwitch.test.tsx (2a.20, 2c.53, 2c.54). The colours of 69, the wrapping of
// 77 and the focus ring of 79 are checked in the designer's review, not in jsdom.
import {
  act,
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
  type BffOptions,
} from './test/bff.ts'
import {
  MODEL_PROFILE_CARDS,
  fieldTabStops,
  pageStartTabStops,
  saveBarTabStops,
} from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const STRATA = 'Strata'
const MODEL =
  '/var/lib/private-ai/.llama.cpp/models/unsloth/Qwen3.6-35B-A3B-GGUF/Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf'
const DRAFT = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0-draft.gguf'
const MMPROJ = '/var/lib/private-ai/.llama.cpp/models/mmproj-F16.gguf'
const URL = 'https://huggingface.co/unsloth/Qwen3.6-35B-A3B-GGUF'
const NONE = en('model-profile-value-none')

/** The Assets card's labels, in the GUI's order (criterion 73). */
const LABELS = [
  en('model-profile-model-file'),
  en('model-profile-project-url'),
  // Not a string: an acronym, as the GUI labels it.
  'MMProj',
  en('model-profile-draft-model'),
  en('model-profile-drafter'),
  en('model-profile-ngram-drafters'),
]

/** The 2e cards' fields, in card and field order: each is one tab stop, its
 * About toggle (criterion 91), and nothing else of a 2e card takes focus
 * (criterion 85). The list is test/modelProfileCards.ts. H11a.3 adds the Assets
 * disclosure first: the card sits between the Engine field and the 2e cards, so
 * its toggle is the tab stop right after the Engine select. */
const CARD_TAB_STOPS = [
  en('model-profile-assets'),
  ...fieldTabStops(fullDoc().values),
  ...saveBarTabStops(),
]

/** The five group headings of the Speculative decoding card of 2e.5 (criterion
 * 114): the strings text of `model-profile-draft-model-group`, and the four speculator
 * names as the INI literals they are. Each opens a group of criterion 90 as an
 * `h4` inside its card, so the page's heading outline carries the five after
 * the last card's `h3`, in the order of 112. */
const SPEC_GROUP_HEADINGS = [
  en('model-profile-draft-model-group'),
  'ngram-mod',
  'ngram-simple',
  'ngram-map-k',
  'ngram-map-k4v',
]

/** A model profile with every Assets value set (82), as the file holds it, plus keys
 * the page does not show. `extra` replaces a top-level member, `projectUrl`
 * included. */
function fullDoc(extra: Record<string, unknown> = {}) {
  return modelProfileDoc(
    {
      model: MODEL,
      mmproj: MMPROJ,
      'model-draft': DRAFT,
      'spec-type': 'ngram-mod,draft-mtp',
      'ctx-size': '172032',
      temp: '0.7',
    },
    { projectUrl: URL, ...extra },
  )
}

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfilePage.test.tsx) ────────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

async function heading1(main: HTMLElement, name: string): Promise<HTMLElement> {
  return within(main).findByRole('heading', { level: 1, name })
}

/** Waits for the loaded page's content; the card is the Assets Card. */
async function loaded(options: BffOptions = {}, name = ID) {
  const { bff, main } = open(options)
  const h1 = await heading1(main, name)
  const h3 = await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  const card = h3.closest('.card')
  expect(card, 'the Assets heading sits in a Card').toBeTruthy()
  return { bff, main, h1, h3, card: card as HTMLElement }
}

async function message(main: HTMLElement, text: string): Promise<HTMLElement> {
  return within(main).findByText(text)
}

function settingsTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
}

/** The section jump list of 2h.10's 196, found by its name. */
function navOf(main: HTMLElement): HTMLElement {
  return within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
}

/** The engine Notice of 72, found by its text: 2h.10's 194 took the read-only
 * Notice away, so it is the page's only one. */
function engineNotice(main: HTMLElement): HTMLElement {
  const want = en('model-profile-engine-unavailable', STRATA)
  const found = [...main.querySelectorAll('.notice')].filter(
    (n) => text(n) === want,
  )
  expect(found.length, `the Notice model-profile-engine-unavailable`).toBe(1)
  return found[0] as HTMLElement
}

/** The texts of every Notice on the page. */
function noticeTexts(main: HTMLElement): string[] {
  return [...main.querySelectorAll('.notice')].map(text)
}

function engineSelect(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('combobox', {
    name: en('model-profile-engine'),
  })
}

function aboutToggle(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('button', {
    name: `${en('field-about')} ${label}`,
  })
}

function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** H11a.4: the Assets card is closed on mount and its rows sit behind
 * `hidden`, which closes the role queries to them, so every test that reads the
 * description list opens the card first. */
function openCard(card: HTMLElement): void {
  const toggle = card.querySelector('h3 button')
  expect(toggle, 'H11a.3: the disclosure in the card h3').not.toBeNull()
  fireEvent.click(toggle as HTMLElement)
}

/** The summary line H11a.1 renders under the h1. */
function summaryLine(main: HTMLElement): HTMLElement {
  const line = main.querySelector('p.model-profile-summary')
  expect(line, 'H11a.1: the summary line').not.toBeNull()
  return line as HTMLElement
}

function terms(card: HTMLElement): HTMLElement[] {
  return within(card).getAllByRole('term')
}

/** The card's `<dt>`/`<dd>` pairs, as [label, value]. */
function pairs(card: HTMLElement): [string, string][] {
  return definitionsWithLabels(card).map(([label, def]) => [label, text(def)])
}

/** The card's `<dt>`/`<dd>` pairs, with the value's element. */
function definitionsWithLabels(card: HTMLElement): [string, HTMLElement][] {
  const ts = terms(card)
  const ds = within(card).getAllByRole('definition')
  expect(ts.length, 'one definition for each term').toBe(ds.length)
  return ts.map((el, i) => [text(el), ds[i]!])
}

/** The classes on a value's element and on the child that carries the font. */
function classesOf(def: Element): string {
  const inner = def.querySelector('.mono, .muted')
  return `${def.className} ${inner?.className ?? ''}`
}

/** The elements Tab reaches (no positive tabindex is used). */
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

/** The names Tab reaches inside `main`. */
function mainTabbable(main: HTMLElement): string[] {
  return tabbable()
    .filter((el) => main.contains(el))
    .map(nameOf)
}

/** The name Tab shows: a control's text, or the label that names an input. */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  const aria = el.getAttribute('aria-label')
  if (aria) return aria
  const id = el.getAttribute('id')
  const label = id ? document.querySelector(`label[for="${id}"]`) : null
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const wrap = el.closest('label')
  return (label?.textContent ?? wrap?.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

// ── Failure paths first: the reply shape the fe checks (81) ─────────────

test('2d.81: a key of values the page does not show is ignored, and a values whose value is a number is model-profile-load-failed with no card', async () => {
  const { card } = await loaded({
    modelProfile: () =>
      reply(
        200,
        fullDoc({
          values: {
            model: MODEL,
            mmproj: MMPROJ,
            'model-draft': DRAFT,
            'spec-type': 'draft-eagle3',
            'ctx-size': '172032',
            temp: 'zz-secret',
          },
        }),
      ),
  })
  // H11a.4: the card mounts closed, so open it before reading the rows
  // (as 2d.73 to 2d.76 and 2d.79 do).
  openCard(card)
  expect(pairs(card).map(([label]) => label)).toEqual(LABELS)
  expect(card.textContent).not.toContain('ctx-size')
  expect(card.textContent).not.toContain('172032')
  expect(card.textContent).not.toContain('temp')
  expect(card.textContent).not.toContain('zz-secret')
  cleanup()
  vi.unstubAllGlobals()
  // Criterion 81: `values` holds strings only, anything else is the load
  // failure. `loaded()` is not used here: it waits for the Assets heading,
  // which this reply never shows.
  const { main } = open({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { values: { model: 7 } })),
  })
  expect(await message(main, en('model-profile-load-failed'))).toBeTruthy()
  expect(within(main).queryAllByRole('term')).toEqual([])
  expect(within(main).queryByRole('heading', { level: 3 })).toBeNull()
})

test('2d.71 (2h.1 130, 135): the save bar is there to save with: Save changes and Discard, and in main Tab reaches the back link, the ten jump buttons of 2h.10, the Engine About toggle and select, then each 2e field About toggle, the editable controls and the bar', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, fullDoc()) })
  await engineSelect(main)
  for (const name of [en('save-button'), en('save-discard')])
    expect(within(main).queryByRole('button', { name }), name).toBeTruthy()
  expect(main.querySelector('.save-bar'), 'the save bar').toBeTruthy()
  expect(mainTabbable(main)).toEqual([
    ...pageStartTabStops(),
    `${en('field-about')} ${en('model-profile-engine')}`,
    en('model-profile-engine'),
    ...CARD_TAB_STOPS,
  ])
})

// ── The order and the off line (68, 69) ─────────────────────────────────

test("2d.68: below the h1, in order: the summary line of H11a.1, the off line (a switched-off model profile), the h2 Model profile settings, the jump list of 2h.10, the engine Notice (an engine not available), the Engine field, the Assets card, and the headings stay h1, h2, h3 plus the 2e.5 card's five h4", async () => {
  for (const [what, extra, off, unavailable] of [
    ['a switched-on model profile', {}, false, false],
    ['a switched-off model profile', { enabled: false }, true, false],
    [
      'an engine not available',
      { engine: STRATA, engineAvailable: false },
      false,
      true,
    ],
    [
      'switched off and not available',
      { enabled: false, engine: STRATA, engineAvailable: false },
      true,
      true,
    ],
  ] as [string, Record<string, unknown>, boolean, boolean][]) {
    const { main, h1, h3 } = await loaded({
      modelProfile: () => reply(200, fullDoc(extra)),
    })
    // H11a.1: the summary line is the first thing under the h1, before the off
    // line when the profile is switched off.
    const line = summaryLine(main)
    expect(before(h1, line), 'H11a.1: the summary is under the h1').toBe(true)
    const nodes: [string, Node][] = [['the summary line', line]]
    if (off)
      nodes.push(['the off line', await message(main, en('model-profile-off'))])
    else
      expect(
        within(main).queryByText(en('model-profile-off')),
        `${what}: no off line`,
      ).toBeNull()
    nodes.push(['the h2', settingsTitle(main)], ['the jump list', navOf(main)])
    if (unavailable) nodes.push(['the engine Notice', engineNotice(main)])
    nodes.push(['the Engine field', await engineSelect(main)])
    nodes.push(['the Assets card', h3])
    for (let i = 0; i + 1 < nodes.length; i++)
      expect(
        before(nodes[i]![1], nodes[i + 1]![1]),
        `${what}: ${nodes[i]![0]} before ${nodes[i + 1]![0]}`,
      ).toBe(true)
    // 2h.10's 194: the read-only Notice is gone, so the engine Notice of 72 is
    // the only Notice the page has.
    expect(noticeTexts(main), `${what}: the Notices of the page`).toEqual(
      unavailable ? [en('model-profile-engine-unavailable', STRATA)] : [],
    )
    expect(
      within(main)
        .getAllByRole('heading')
        .map((h) => h.getAttribute('aria-level') ?? h.tagName.slice(1))
        .join(' '),
      `${what}: the heading order of D8 (the h1, the h2, one h3 per card of test/modelProfileCards.ts, then the five h4 group headings of 2e.5's last card, of criterion 114)`,
    ).toBe(
      `1 2${' 3'.repeat(MODEL_PROFILE_CARDS.length)}${' 4'.repeat(SPEC_GROUP_HEADINGS.length)}`,
    )
    // 114: the five group headings, in the order of 112, inside the last card.
    const h4 = within(main)
      .getAllByRole('heading', { level: 4 })
      .map((h) => text(h))
    expect(h4, `${what}: the group headings of 114, in order`).toEqual(
      SPEC_GROUP_HEADINGS,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2d.69: a switched-off model profile shows model-profile-off as a plain paragraph under the h1 and the page is not drawn muted; a switched-on model profile has no such line', async () => {
  const { main, h1 } = await loaded({
    modelProfile: () => reply(200, fullDoc({ enabled: false })),
  })
  const line = await message(main, en('model-profile-off'))
  expect(line.tagName).toBe('P')
  expect(line.getAttribute('role'), 'not a status, not an alert').toBeNull()
  expect(line.closest('.notice'), 'not a Notice').toBeNull()
  expect(text(line)).toBe(en('model-profile-off'))
  expect(before(h1, line), 'the line is under the h1').toBe(true)
  for (const el of [main, h1, line])
    expect(
      String(el.className),
      'the state is said in words, not by muting',
    ).not.toContain('muted')
  cleanup()
  vi.unstubAllGlobals()
  const { main: on } = await loaded()
  expect(within(on).queryByText(en('model-profile-off'))).toBeNull()
})

// ── The Engine field (70, 72, 80) ───────────────────────────────────────

test('2d.70: the Engine field is a SelectField named Engine: one option, the engine as the reply gives it, selected, enabled and in the tab sequence, and its About toggle reveals and hides model-profile-engine-hint', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, fullDoc()) })
  const select = (await engineSelect(main)) as HTMLSelectElement
  expect(
    Array.from(select.options).map((o) => o.text),
    'one option',
  ).toEqual(['llama.cpp'])
  expect(select.value, 'the model profile engine, selected').toBe('llama.cpp')
  expect(select.selectedIndex).toBe(0)
  expect(select.disabled, 'enabled: 2h will let it change').toBe(false)
  expect(select.getAttribute('aria-disabled')).toBeNull()
  expect(tabbable(), 'in the tab sequence').toContain(select)
  const toggle = aboutToggle(main, en('model-profile-engine'))
  expect(toggle.getAttribute('aria-expanded')).toBe('false')
  const hintId = toggle.getAttribute('aria-controls')
  const hint = hintId === null ? null : document.getElementById(hintId)
  expect(hint, 'the toggle controls the hint').toBeTruthy()
  expect(isHidden(hint), 'the hint starts hidden').toBe(true)
  fireEvent.click(toggle)
  expect(toggle.getAttribute('aria-expanded')).toBe('true')
  expect(isHidden(hint), 'the hint is shown').toBe(false)
  expect(
    within(hint as HTMLElement).getByText(en('model-profile-engine-hint')),
  ).toBeTruthy()
  fireEvent.click(toggle)
  expect(toggle.getAttribute('aria-expanded')).toBe('false')
  expect(isHidden(hint), 'the hint is hidden again').toBe(true)
})

test('2d.70, 2d.72: an engine not available: the one option reads Strata (not available) and is selected, the Notice model-profile-engine-unavailable filled with the engine sits above the Engine field as a bordered paragraph and not a live region, and the Assets card is shown as for any model profile', async () => {
  const { main, card } = await loaded({
    modelProfile: () =>
      reply(200, fullDoc({ engine: STRATA, engineAvailable: false })),
  })
  openCard(card)
  const select = (await engineSelect(main)) as HTMLSelectElement
  expect(Array.from(select.options).map((o) => o.text)).toEqual([
    en('model-profiles-engine-not-available', STRATA),
  ])
  expect(select.selectedIndex).toBe(0)
  const n = engineNotice(main)
  expect(n.tagName).toBe('P')
  expect(
    n.getAttribute('role'),
    'part of the page as read, not a live region',
  ).toBeNull()
  expect(text(n)).toBe(en('model-profile-engine-unavailable', STRATA))
  expect(before(n, select), 'the Notice is above the Engine field').toBe(true)
  expect(pairs(card).map(([label]) => label)).toEqual(LABELS)
})

test('2d.70, 2d.80: picking the Engine select only option changes nothing and sends nothing: no write, no second read, no engines read, no list read, and the values stay', async () => {
  const { bff, main, card } = await loaded({
    modelProfile: () => reply(200, fullDoc()),
  })
  openCard(card)
  const select = (await engineSelect(main)) as HTMLSelectElement
  const shown = pairs(card)
  const seen = bff.calls.map((c) => `${c.method} ${c.path}`)
  await act(async () => {
    fireEvent.click(select)
    fireEvent.change(select, { target: { value: 'llama.cpp' } })
    await new Promise((r) => setTimeout(r, 0))
  })
  expect(
    bff.calls.map((c) => `${c.method} ${c.path}`),
    'nothing is sent',
  ).toEqual(seen)
  expect(
    seen.filter((c) => /^(POST|PUT|DELETE) /.test(c)),
    'nothing to write in 2d',
  ).toEqual([])
  expect(
    seen.filter((c) => c.startsWith('GET /api/profiles/model')),
    'the model profile read once, the list not read (80)',
  ).toEqual(['GET /api/profiles/model/Qwen3-27B'])
  expect(seen, 'no engine list read in 2d').not.toContain('GET /api/engines')
  expect(select.value).toBe('llama.cpp')
  expect(pairs(card), 'the values stay').toEqual(shown)
})

// ── The Assets card (73, 74, 75, 76) ───────────────────────────────────

test('2d.73: the Assets card is a Card with an h3 Assets and one dl: each field a dt label then its dd value, in the GUI order Model file, Project URL, MMProj, Draft model, Drafter, N-gram drafters', async () => {
  const { card, h3 } = await loaded({
    modelProfile: () => reply(200, fullDoc()),
  })
  openCard(card)
  expect(
    within(card).getByRole('heading', {
      level: 3,
      name: en('model-profile-assets'),
    }),
  ).toBe(h3)
  expect(card.querySelectorAll('dl').length, 'one description list').toBe(1)
  const ds = within(card).getAllByRole('definition')
  const ts = terms(card)
  expect(ts.map(text)).toEqual(LABELS)
  expect(ts.map((el) => el.tagName)).toEqual(LABELS.map(() => 'DT'))
  expect(ds.map((el) => el.tagName)).toEqual(LABELS.map(() => 'DD'))
  for (const t of ts) expect(t.parentElement?.tagName, text(t)).toBe('DL')
  ts.forEach((t, i) =>
    expect(
      t.nextElementSibling,
      `${LABELS[i]}: its value follows its label`,
    ).toBe(ds[i]),
  )
  expect(pairs(card)).toEqual([
    [en('model-profile-model-file'), MODEL],
    [en('model-profile-project-url'), URL],
    ['MMProj', MMPROJ],
    [en('model-profile-draft-model'), DRAFT],
    [en('model-profile-drafter'), 'draft-mtp'],
    [en('model-profile-ngram-drafters'), 'ngram-mod'],
  ])
})

test('2d.73 (H11a.3, H11a.5): the Assets card holds one button, the disclosure in its h3, and its body holds no input, picker, checkbox, button, select, link or hint, so nothing in the body is in the tab sequence', async () => {
  const { card } = await loaded({ modelProfile: () => reply(200, fullDoc()) })
  const toggle = card.querySelector('h3 button')
  expect(toggle, 'H11a.3: the disclosure is a button in the h3').not.toBeNull()
  expect(within(card).getAllByRole('button'), 'one button').toEqual([toggle])
  // H11a.4: open, so the role queries below reach the rows.
  openCard(card)
  const found = card.querySelector<HTMLElement>('#model-profile-assets-body')
  expect(found, 'H11a.3: the body div').not.toBeNull()
  const body = found as HTMLElement
  for (const role of [
    'button',
    'textbox',
    'combobox',
    'checkbox',
    'spinbutton',
    'searchbox',
    'link',
  ] as const)
    expect(within(body).queryAllByRole(role), role).toEqual([])
  expect(
    body.querySelectorAll('input, select, textarea, button, a').length,
    'the body holds no control (H11a.5)',
  ).toBe(0)
  expect(body.querySelectorAll('.field-hint').length, 'no hints in 2d').toBe(0)
  expect(
    tabbable().filter((el) => body.contains(el)),
    'the body: nothing in the tab sequence (H11a.5)',
  ).toEqual([])
  expect(
    tabbable().filter((el) => card.contains(el)),
    'the card: the toggle is its only tab stop',
  ).toEqual([toggle])
})

test('2d.74: each value is the reply string unchanged, the server path in full with its separators, in the mono font, and the Project URL is text, not a link', async () => {
  const { card } = await loaded({ modelProfile: () => reply(200, fullDoc()) })
  // H11a.4: the rows sit behind `hidden` until the card is opened.
  openCard(card)
  const shown = new Map(pairs(card))
  expect(
    shown.get(en('model-profile-model-file')),
    'no file-name shortening',
  ).toBe(MODEL)
  expect(shown.get(en('model-profile-draft-model'))).toBe(DRAFT)
  expect(shown.get('MMProj')).toBe(MMPROJ)
  expect(shown.get(en('model-profile-project-url'))).toBe(URL)
  for (const [label, def] of definitionsWithLabels(card)) {
    if (text(def) === NONE) continue
    expect(classesOf(def), `${label}: a value is mono (D5.1)`).toContain('mono')
  }
  expect(within(card).queryAllByRole('link'), 'no Open in 2d').toEqual([])
  expect(card.querySelectorAll('a[href]').length).toBe(0)
})

test('2d.75: spec-type is split as model_profiles.rs does: Drafter is the first entry that does not start with ngram-, N-gram drafters are the ngram- entries in the value order joined with a comma and a space, empty entries and none dropped', async () => {
  for (const [specType, drafter, ngrams] of [
    ['ngram-mod,draft-mtp', 'draft-mtp', 'ngram-mod'],
    ['draft-eagle3', 'draft-eagle3', NONE],
    [' ngram-simple , ngram-map-k ', NONE, 'ngram-simple, ngram-map-k'],
    ['ngram-cache', NONE, 'ngram-cache'],
    ['ngram-mod,draft-mtp,ngram-map-k', 'draft-mtp', 'ngram-mod, ngram-map-k'],
    ['draft-a,ngram-x,draft-b', 'draft-a', 'ngram-x'],
    ['none', NONE, NONE],
    ['', NONE, NONE],
  ] as [string, string, string][]) {
    const { card } = await loaded({
      modelProfile: () =>
        reply(200, fullDoc({ values: { 'spec-type': specType } })),
    })
    openCard(card)
    const shown = new Map(pairs(card))
    expect(shown.get(en('model-profile-drafter')), specType).toBe(drafter)
    expect(shown.get(en('model-profile-ngram-drafters')), specType).toBe(ngrams)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2d.76: a key absent from values, an empty string, an empty projectUrl and a spec-type with no entry of a kind all show (none), muted and in the body font, not mono', async () => {
  for (const [what, doc] of [
    [
      'values {} and projectUrl ""',
      modelProfileDoc({}, { values: {}, projectUrl: '' }),
    ],
    [
      'every Assets key an empty string',
      modelProfileDoc(
        { model: '', mmproj: '', 'model-draft': '', 'spec-type': '' },
        { projectUrl: '' },
      ),
    ],
    [
      'spec-type none',
      modelProfileDoc(
        { model: '', mmproj: '', 'model-draft': '', 'spec-type': 'none' },
        { projectUrl: '' },
      ),
    ],
  ] as [string, ReturnType<typeof modelProfileDoc>][]) {
    const { card } = await loaded({ modelProfile: () => reply(200, doc) })
    openCard(card)
    const defs = definitionsWithLabels(card)
    expect(
      defs.map(([label]) => label),
      what,
    ).toEqual(LABELS)
    for (const [label, def] of defs) {
      expect(text(def), `${what}: ${label}`).toBe(NONE)
      expect(classesOf(def), `${what}: ${label} reads as empty`).toContain(
        'muted',
      )
      expect(classesOf(def), `${what}: ${label} is not a path`).not.toContain(
        'mono',
      )
    }
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── Keyboard and screen reader (78, 79) ─────────────────────────────────

test('2d.78: Tab on the model profile page goes skip link, banner, nav, the back link, the ten jump buttons of 2h.10 in card order, the Engine About toggle, the Engine select, then each 2e field About toggle, and nothing else on the page takes focus', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, fullDoc()) })
  await engineSelect(main)
  expect(tabbable().map(nameOf)).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    // Brief 07 H4.2: the theme switch, between Menu and the session part.
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-model-profiles'),
    ...pageStartTabStops(),
    `${en('field-about')} ${en('model-profile-engine')}`,
    en('model-profile-engine'),
    ...CARD_TAB_STOPS,
  ])
  // The h1 takes the focus only from the page (tabindex -1): Tab starts below it.
  expect(
    screen
      .getByRole('heading', { level: 1, name: ID })
      .getAttribute('tabindex'),
  ).toBe('-1')
})

test('2d.79: a screen reader: the h1 is the id at level 1, the select is named Engine and says llama.cpp, the card is a heading over a description list where each value follows its term, and nothing is announced on arrival', async () => {
  const { main, card, h1 } = await loaded({
    modelProfile: () => reply(200, fullDoc()),
  })
  openCard(card)
  expect(h1.tagName).toBe('H1')
  expect(text(h1)).toBe(ID)
  const select = (await engineSelect(main)) as HTMLSelectElement
  expect(
    select.options[select.selectedIndex]!.text,
    'combo box, llama.cpp',
  ).toBe('llama.cpp')
  const [first, second] = pairs(card)
  expect(first, 'Model file, then the path').toEqual([
    en('model-profile-model-file'),
    MODEL,
  ])
  expect(second).toEqual([en('model-profile-project-url'), URL])
  // The loading status went with the content. 2h.1's 135 keeps the save bar's
  // status line on the page permanently: it is the only live region, it is
  // empty on arrival, so the page still announces nothing.
  const statuses = within(main).queryAllByRole('status')
  expect(
    statuses,
    'the loading status is gone, the bar line is the only one left',
  ).toHaveLength(1)
  const line = statuses[0]!
  expect(
    line.closest('.save-bar'),
    'the line is the save bar of 135',
  ).toBeTruthy()
  expect(text(line), 'empty on arrival: nothing announced').toBe('')
  expect(
    [...main.querySelectorAll('[role="alert"], [role="log"], [aria-live]')],
    'no alert, no log, no explicit live region on arrival (a NodeList is not deep-equal to [])',
  ).toEqual([])
})

// ── The ids of 82 on the content ────────────────────────────────────────

test('2d.80, 2d.82: an id with a space and one with a % open on their content, read once with the id encoded as one path segment', async () => {
  for (const [id, encoded] of [
    ['Qwen 27B', 'Qwen%2027B'],
    ['q%bar', 'q%25bar'],
  ] as const) {
    startAt(`#/profiles/model/${encoded}`)
    const { bff, main } = open()
    expect(await heading1(main, id), id).toBeTruthy()
    const h3 = await within(main).findByRole('heading', {
      level: 3,
      name: en('model-profile-assets'),
    })
    const card = h3.closest('.card') as HTMLElement
    openCard(card)
    expect(terms(card).map(text), id).toEqual(LABELS)
    expect(bff.to('GET', `/api/profiles/model/${encoded}`).length, id).toBe(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})
