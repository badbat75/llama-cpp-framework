// Hearth H11b, the Engine field becoming the first row of the Assets card
// (fe/briefs/08-hearth-h10-h11.md, section 3). H11b.1 to H11b.4 are pinned
// here on the rendered page; H11b.5 is the edits to the existing test files,
// not a test of its own; H11b.6 is the card read aloud, and H11b.7's "nothing
// else moves" is what the other files pin, so both are marked OPEN here as
// the brief's section 4 leaves them to the UX pass and to the 2d and 2e suites.
// H11b names no stylesheet declaration, so this file reads no CSS text.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test } from 'vitest'
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
import {
  cardHeadings,
  fieldAboutToggles,
  saveBarTabStops,
} from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const ASSETS = en('model-profile-assets')
const ENGINE = en('model-profile-engine')
const ENGINE_VALUE = 'llama.cpp'
const STRATA = 'Strata'
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'
const DRAFT = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0-draft.gguf'
const MMPROJ = '/var/lib/private-ai/.llama.cpp/models/mmproj-F16.gguf'
const URL = 'https://huggingface.co/org/repo'
/** H11a.3's ids, the ones H11b keeps: the heading, the toggle, the body. */
const HEADING_ID = 'model-profile-card-assets'
const TOGGLE_ID = 'model-profile-assets-toggle'
const BODY_ID = 'model-profile-assets-body'
/** H11b.3: the key the slice removes. Named as a literal: `en` of it throws. */
const HINT_KEY = 'model-profile-engine-hint'
/** The card headings in the page order of 83 (the ten jump buttons' names). */
const TITLES = cardHeadings()

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  startAt('')
})

/** The read every test starts from: every Assets row filled, so a row that
 * reads `(none)` is a miss and not a fixture. `extra` replaces top-level
 * members of the document. */
function assetsDoc(extra: Record<string, unknown> = {}) {
  return modelProfileDoc(
    {
      model: MODEL,
      mmproj: MMPROJ,
      'model-draft': DRAFT,
      'spec-type': 'ngram-mod,draft-mtp',
      'ctx-size': '172032',
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

function byId(id: string, what: string): HTMLElement {
  const el = document.getElementById(id)
  expect(el, `${what}: #${id}`).not.toBeNull()
  return el as HTMLElement
}

const textOf = (el: Element): string =>
  (el.textContent ?? '').replace(/\s+/g, ' ').trim()

/** Opens the Assets card, the only way the rows are read by a role query
 * (H11a.4: closed, the body carries `hidden`). */
function openCard(): void {
  fireEvent.click(byId(TOGGLE_ID, 'the Assets disclosure'))
}

/** The card's pairs from the DOM, as the closed body hides them from the
 * role queries (the reader of HearthAssets.test.tsx). */
function pairs(): [string, string][] {
  const dls = byId(BODY_ID, 'the Assets body').querySelectorAll('dl')
  expect(dls.length, 'one description list').toBe(1)
  const out: [string, string][] = []
  for (const dt of dls[0]!.querySelectorAll('dt')) {
    const dd = dt.nextElementSibling
    expect(dd, `${textOf(dt)}: its value follows`).not.toBeNull()
    out.push([textOf(dt), textOf(dd as HTMLElement)])
  }
  return out
}

/** The Engine row's `dd`, the first pair of the card's list. */
function engineDd(): HTMLElement {
  const dd = byId(BODY_ID, 'the Assets body').querySelector('dl > dt')
    ?.nextElementSibling as HTMLElement | undefined
  expect(dd, 'the first dd follows the first dt').not.toBeNull()
  return dd as HTMLElement
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

/** The name Tab shows: a control's text, or the label that names an input
 * (the reader of ModelProfileContent.test.tsx). */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName)) return textOf(el)
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
  return textOf((label ?? wrap) as Element)
}

describe('H11b.1: the Engine row of Assets', () => {
  test("H11b.1: the Assets dl leads with Engine and its engine id, the dd without mono, the six rows follow in today's order, and the SelectField, its About toggle and its hint are gone", async () => {
    const { main } = await form()
    expect(pairs()[0], 'closed: Engine, llama.cpp').toEqual([
      ENGINE,
      ENGINE_VALUE,
    ])
    openCard()
    expect(pairs(), 'the seven pairs, Engine first').toEqual([
      [ENGINE, ENGINE_VALUE],
      [en('model-profile-model-file'), MODEL],
      [en('model-profile-project-url'), URL],
      ['MMProj', MMPROJ],
      [en('model-profile-draft-model'), DRAFT],
      [en('model-profile-drafter'), 'draft-mtp'],
      [en('model-profile-ngram-drafters'), 'ngram-mod'],
    ])
    const dd = engineDd()
    expect(textOf(dd), 'the dd is the engine').toBe(ENGINE_VALUE)
    // H11b.1: no `mono`: the engine id is not a path, it sits in the body
    // font. The dd carries no class at all, so a child is not excluded.
    expect([...dd.classList], 'the dd without mono').not.toContain('mono')
    expect(dd.innerHTML, 'the dd carries the engine alone').toBe(ENGINE_VALUE)
    // H11b.1: the SelectField goes, with its About toggle and its hint. The
    // hidden nodes are read from the DOM, as `hidden` closes the roles.
    expect(
      main.querySelector('select#model-profile-engine'),
      'H11b.1: the select with the id is gone',
    ).toBeNull()
    expect(
      main.querySelector('#model-profile-engine'),
      'H11b.1: no element keeps the id',
    ).toBeNull()
    expect(
      within(main).queryByRole('combobox', { name: ENGINE }),
      'H11b.1: the combobox named Engine is gone',
    ).toBeNull()
    expect(
      main.querySelectorAll('select').length,
      'the 2e cards keep their selects: only Engine left the page',
    ).toBeGreaterThan(0)
    expect(
      within(main).queryByRole('button', {
        name: `${en('field-about')} ${ENGINE}`,
      }),
      'H11b.1: the Engine About toggle is gone',
    ).toBeNull()
    expect(
      [...main.querySelectorAll('.field-hint')].some(
        (hint) => textOf(hint) === t('model-profile-engine-hint'),
      ),
      'H11b.1: the hint text is off the page',
    ).toBe(false)
  })

  test('H11b.1, H11b.2: an engine not available: the Engine row reads Strata (not available) and the Notice model-profile-engine-unavailable sits above the closed card, a paragraph and not a live region, while the card stays as for any model profile', async () => {
    const { main } = await form(
      assetsDoc({ engine: STRATA, engineAvailable: false }),
    )
    expect(
      pairs()[0],
      'closed: the row says the engine is not available',
    ).toEqual([ENGINE, en('model-profiles-engine-not-available', STRATA)])
    const notices = [...main.querySelectorAll('p.notice')]
    expect(
      notices.length,
      "H11b.2: the engine Notice is the page's only one",
    ).toBe(1)
    const notice = notices[0] as HTMLElement
    expect(textOf(notice), 'H11b.2: the text of 72').toBe(
      en('model-profile-engine-unavailable', STRATA),
    )
    expect(
      notice.getAttribute('role'),
      'H11b.2: part of the page as read, not a live region',
    ).toBeNull()
    const card = byId(HEADING_ID, "the Assets card's h3").closest('section')
    expect(card, 'the card element').not.toBeNull()
    expect(
      before(notice, card as HTMLElement),
      'H11b.2: the Notice is above the Assets card',
    ).toBe(true)
    expect(
      isHidden(byId(BODY_ID, 'the Assets body')),
      'H11b.2: the card stays closed: the engine is seen with the card closed',
    ).toBe(true)
    openCard()
    expect(
      isHidden(byId(BODY_ID, 'the Assets body')),
      'H11b.2: the card opens as for any model profile',
    ).toBe(false)
  })
})

describe('H11b.3: the strings', () => {
  test('H11b.3: model-profile-engine stays as the dt and model-profile-engine-hint is out of the test table, the fe strings file and the sources', () => {
    expect(ENGINE, 'H11b.3: model-profile-engine kept, now the dt').toBe(
      'Engine',
    )
    expect(
      () => en(HINT_KEY),
      'H11b.3: the key is out of src/test/bff.ts EN',
    ).toThrow(`no English for ${HINT_KEY} in the test`)
    const table = JSON.parse(
      readFileSync(join(import.meta.dirname, 'strings', 'en_US.json'), 'utf8'),
    ) as Record<string, string>
    expect(
      table[HINT_KEY],
      'H11b.3: the key is out of en_US.json',
    ).toBeUndefined()
    expect(
      t(HINT_KEY),
      'H11b.3: t of the removed key answers the key itself',
    ).toBe(HINT_KEY)
    // No source of the page may keep referencing the key: the reverse lint of
    // strings.test.ts. Named as a literal here, so this file does not match.
    const sources = readFileSync(
      join(import.meta.dirname, 'ModelProfilePage.tsx'),
      'utf8',
    )
    expect(
      sources,
      'H11b.3: the page no longer references the key',
    ).not.toContain(`${HINT_KEY}'`)
  })
})

describe('H11b.4: the tab order', () => {
  test("H11b.4 (22 D2.5, 22 D4.10): Tab in main goes the back link, the save bar with its Delete, the jump toggle, the ten jumps, the Assets toggle, then the first field's About toggle of 91: the Engine About toggle and select are out", async () => {
    const { main } = await form()
    const names = tabbableElements(main).map(nameOf)
    const start = [
      en('model-profile-back'),
      // 22 D2.5 moved the bar to the top of the settings section, so its stops
      // sit right after the back link, before the jump list. 22 D4.10 puts the
      // bar's Delete first in it.
      ...saveBarTabStops(),
      en('model-profile-jump-label'),
      ...TITLES,
      ASSETS,
      // The first field's About toggle of 91: the first card of 83 after
      // Assets is the Hardware card, so its first field.
      fieldAboutToggles()[0],
    ]
    expect(names.slice(0, start.length), 'the page start, H11b.4').toEqual(
      start,
    )
    expect(
      names.filter(
        (n) => n === ENGINE || n === `${en('field-about')} ${ENGINE}`,
      ),
      'H11b.4: no Engine stop is left',
    ).toEqual([])
  })
})
