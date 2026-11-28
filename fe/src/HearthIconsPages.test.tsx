// fe/src/HearthIconsPages.test.tsx
// H8b, the Lucide icons of the pages (fe/briefs/08-hearth-h8-icons.md), the DOM
// plane: H8b.1 the run verbs, H8b.2 the back link, H8b.3 the four notices,
// H8b.4 the filter field (the CSS of H8b.4 and all of H8b.5 are stylesheet
// text, in HearthH8b.test.ts), H8b.6 the field error's drawn icon replaced by
// `Icon circle-alert`, H8b.7 the sweep over every `svg` of the five pages and
// H8b.8 that no icon is alone.
//
// The whole app renders against the fake bff (mocked fetch), on the real clock:
// nothing here waits on a poll time. Every icon is decorative, so no accessible
// name moves: the run verbs are still found by the name RunControls.test.tsx
// uses, and a notice's text is what it was, an `svg` adding no text.
//
// H8b.9 (no key, focus moves, the live regions) is carried by the tests of the
// pages the slice may not move: RunControls.test.tsx, ModelProfilesFilter.test.tsx,
// ModelProfileFlags.test.tsx, SessionRevoked.test.tsx, InsecureSignIn.test.tsx
// and LeaveGuard.test.tsx.
//
// OPEN, for the designer's UX pass (section 4): the icon lined up with the text
// beside it, the search icon inside the field and not under typed text, and what
// any of it looks like in forced colours. jsdom lays nothing out.
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
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

// ── The shapes of the brief's list, as data ───────────────────────────────
// A hand-typed copy of the brief's shapes block (section 1), the same table as
// HearthIcons.test.tsx: a test file imports no other test file. Only the shapes
// H8b places are here, and the attributes of section 0.
type Shape = [tag: string, attrs: Record<string, string>]

const SHAPES: Record<string, Shape[]> = {
  play: [
    [
      'path',
      {
        d: 'M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z',
      },
    ],
  ],
  square: [['rect', { width: '18', height: '18', x: '3', y: '3', rx: '2' }]],
  'rotate-cw': [
    ['path', { d: 'M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8' }],
    ['path', { d: 'M21 3v5h-5' }],
  ],
  'chevron-left': [['path', { d: 'm15 18-6-6 6-6' }]],
  search: [
    ['path', { d: 'm21 21-4.34-4.34' }],
    ['circle', { cx: '11', cy: '11', r: '8' }],
  ],
  info: [
    ['circle', { cx: '12', cy: '12', r: '10' }],
    ['path', { d: 'M12 16v-4' }],
    ['path', { d: 'M12 8h.01' }],
  ],
  'triangle-alert': [
    [
      'path',
      {
        d: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3',
      },
    ],
    ['path', { d: 'M12 9v4' }],
    ['path', { d: 'M12 17h.01' }],
  ],
  'lock-open': [
    ['rect', { width: '18', height: '11', x: '3', y: '11', rx: '2', ry: '2' }],
    ['path', { d: 'M7 11V7a5 5 0 0 1 9.9-1' }],
  ],
  'circle-alert': [
    ['circle', { cx: '12', cy: '12', r: '10' }],
    ['line', { x1: '12', x2: '12', y1: '8', y2: '12' }],
    ['line', { x1: '12', x2: '12.01', y1: '16', y2: '16' }],
  ],
}

// The eight attributes every icon carries (section 0). No `width`/`height` (CSS
// sizes it), no `title`, no label prop: every icon is decorative.
const ICON_ATTRS: Record<string, string> = {
  'aria-hidden': 'true',
  focusable: 'false',
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': '2',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
}

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** `[tag, every attribute]` of an element: the shape of one child of an icon. */
function childOf(el: Element): Shape {
  return [
    el.tagName.toLowerCase(),
    Object.fromEntries([...el.attributes].map((a) => [a.name, a.value])),
  ]
}

/**
 * `host`'s first element child is the icon of `name`: an `svg.icon` with the
 * eight attributes, the shapes of the table, and no text of its own. H8b places
 * the icon first in every placement except the filter row, where the brief
 * places it first as well (section 1: "the icon is the host's first child in
 * every placement except `search`", which is first among the elements).
 */
function expectIcon(host: Element, name: string, why: string): void {
  const icon = host.children[0]
  expect(icon, `${why}: the host has a first element child`).toBeTruthy()
  expect(icon!.tagName.toLowerCase(), `${why}: the icon is an svg`).toBe('svg')
  expect(
    icon!.getAttribute('class'),
    `${why}: of the class the stylesheet sizes (.icon)`,
  ).toBe('icon')
  for (const [key, value] of Object.entries(ICON_ATTRS))
    expect(icon!.getAttribute(key), `${why}: the svg's ${key}`).toBe(value)
  expect(
    [...icon!.children].map(childOf),
    `${why}: the children of the brief's shapes block, in order`,
  ).toEqual(SHAPES[name])
  expect(text(icon!), `${why}: the icon draws, it holds no text`).toBe('')
}

// ── The pages ─────────────────────────────────────────────────────────────

const ID = 'Qwen3-27B'
const PROFILE = `#/profiles/model/${ID}`

// 22 IP5.5 moved the Fit to VRAM card off this page, and the Fit row is the
// only one whose value is refused as it is read (128). The field error this
// file draws on is therefore made the way 163 makes one on a stay key of that
// slice: Top-K typed a value that is not a whole number, then Save changes.
const WHOLE = en('field-error-whole-number')
const TOP_K = 'Top-K'

/** The page's Save changes button. */
const saveButton = (main: HTMLElement): HTMLButtonElement =>
  within(main).getByRole('button', { name: en('save-button') })

/** Marks Top-K: the field carries the error of 163, and the bff is asked
 *  nothing (the client check refuses before the write). */
async function markTopK(main: HTMLElement) {
  const box = (await within(main).findByRole('textbox', {
    name: TOP_K,
  })) as HTMLInputElement
  fireEvent.change(box, { target: { value: 'abc' } })
  fireEvent.click(saveButton(main))
  await within(main).findByText(WHOLE)
}

/** The bff ends the session: 403, key session-not-administrator (S1). */
const REVOKED = () => reply(403, { key: 'session-not-administrator', args: [] })

/** The app at `hash`, the fake bff answering `options`; `main` of the page. */
function openAt(hash: string, options: BffOptions = {}): HTMLElement {
  startAt(hash)
  fakeBff(options)
  render(<App />)
  return screen.getByRole('main')
}

/** Sets the browser's `window.isSecureContext`; `undefined` removes it (I9). */
function secure(value: boolean | undefined): void {
  const w = window as unknown as { isSecureContext?: boolean }
  if (value === undefined) delete w.isSecureContext
  else
    Object.defineProperty(window, 'isSecureContext', {
      value,
      configurable: true,
    })
}

/** The five pages of H8b.7 and H8b.8, each with the wait that says it loaded. */
type PageCase = {
  what: string
  hash: string
  options: BffOptions
  ready: (main: HTMLElement) => Promise<unknown>
  /** What brings the page to its state past being read (22 IP5.5: a model
   *  profile's field error is made, not read). */
  dirty?: (main: HTMLElement) => Promise<unknown>
}

const PAGES: PageCase[] = [
  {
    what: 'the Overview',
    hash: '#/',
    options: {},
    ready: () => screen.findByRole('link', { name: en('nav-overview') }),
  },
  {
    what: 'the sign-in page',
    hash: '#/sign-in',
    options: { session: 'signed-out' },
    ready: (main) =>
      within(main).findByRole('textbox', { name: en('signin-username') }),
  },
  {
    what: 'the instance page',
    hash: '#/instances/default',
    options: {},
    ready: () => screen.findByRole('button', { name: en('run-start') }),
  },
  {
    what: 'the model profiles page',
    hash: '#/profiles/model',
    options: {},
    ready: (main) =>
      within(main).findByRole('table', { name: en('model-profiles-title') }),
  },
  {
    what: 'a model profile with a field error',
    hash: PROFILE,
    options: {},
    ready: (main) => within(main).findByRole('textbox', { name: TOP_K }),
    dirty: markTopK,
  },
]

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  secure(undefined)
  startAt('')
})

// ── H8b.1: the run verbs ──────────────────────────────────────────────────

test('H8b.1: Start, Stop and Restart hold Icon play, square and rotate-cw before their words, an aria-disabled verb keeps its icon, and the names RunControls finds them by are unchanged', async () => {
  const main = openAt('#/instances/default')
  const verbs = [
    ['run-start', 'play'],
    ['run-stop', 'square'],
    ['run-restart', 'rotate-cw'],
  ] as const
  for (const [key, shape] of verbs) {
    const button = await within(main).findByRole('button', { name: en(key) })
    expectIcon(button, shape, `the ${en(key)} button`)
    expect(
      button.firstChild,
      `${en(key)}: the icon is the button FIRST child, text nodes included (the order of H8b.1)`,
    ).toBe(button.children[0])
    expect(
      text(button),
      `${en(key)}: the words are the string, so the name is unchanged`,
    ).toBe(en(key))
  }
  const row = main.querySelector('.run-actions')
  expect(row, 'the row of the three verbs').toBeTruthy()
  expect(
    [...row!.children].map(text),
    'the three, in the order of the page: Start, Stop, Restart',
  ).toEqual([en('run-start'), en('run-stop'), en('run-restart')])
  expect(
    row!.querySelectorAll('svg.icon').length,
    'one icon per verb, nothing else drawn in the row',
  ).toBe(3)
  expect(
    within(main)
      .getByRole('button', { name: en('run-start') })
      .getAttribute('aria-disabled'),
    'the instance is active: Start is the aria-disabled verb here',
  ).toBe('true')

  // The verbs stay drawn while they are switched off: the inactive instance,
  // where Stop and Restart carry `aria-disabled`.
  cleanup()
  const coder = openAt('#/instances/coder')
  for (const [key, shape] of [
    ['run-stop', 'square'],
    ['run-restart', 'rotate-cw'],
  ] as const) {
    const button = await within(coder).findByRole('button', { name: en(key) })
    expect(
      button.getAttribute('aria-disabled'),
      `${en(key)}: the verb is switched off, and drawn`,
    ).toBe('true')
    expectIcon(
      button,
      shape,
      `the aria-disabled ${en(key)} button of the inactive instance`,
    )
    expect(text(button), `${en(key)}: its words, unchanged`).toBe(en(key))
  }
})

// ── H8b.2: the back link ──────────────────────────────────────────────────

test('H8b.2: the back link of a model profile holds Icon chevron-left then its words, and still goes to the Model profiles page', async () => {
  const main = openAt(PROFILE)
  await within(main).findByRole('heading', { level: 1, name: ID })
  const line = main.querySelector('.model-profile-back') as HTMLElement | null
  expect(line, 'the paragraph of the back line').toBeTruthy()
  const link = within(line!).getByRole('link', {
    name: en('model-profile-back'),
  })
  expectIcon(link, 'chevron-left', 'the back link')
  expect(
    link.firstChild,
    'the icon is the link FIRST child, text nodes included',
  ).toBe(link.children[0])
  expect(
    text(link),
    'the name is the string, so the name the screen reader reads is unchanged',
  ).toBe(en('model-profile-back'))
  expect(link.getAttribute('href'), 'and it leads back to the list').toBe(
    '#/profiles/model',
  )
  expect(
    line!.querySelectorAll('svg').length,
    'one icon in the back line',
  ).toBe(1)
})

// ── H8b.3: the notices ────────────────────────────────────────────────────

test('H8b.3 sign-in: the signin-required Notice holds Icon info, the session-revoked one Icon triangle-alert with its role, the signin-insecure one Icon lock-open, and no notice text moves', async () => {
  // signin-required: a protected page while signed out (1a.10).
  const signedOut = openAt('#/instances/default', { session: 'signed-out' })
  const required = await within(signedOut).findByText(en('signin-required'))
  expect(required.classList.contains('notice'), 'the .notice look').toBe(true)
  expect(required.firstChild, 'the icon is the paragraph FIRST child').toBe(
    required.children[0],
  )
  expectIcon(required, 'info', 'the signin-required Notice')
  expect(
    required.textContent,
    'the words are the string exactly: an svg with no text adds none',
  ).toBe(en('signin-required'))
  expect(
    required.getAttribute('role'),
    'a plain Notice, not an alert',
  ).toBeNull()
  cleanup()

  // session-revoked: a read answered 403 session-not-administrator (S1.9).
  // 17a.6: the server document's read is GET /api/profiles/server/<id>, on the
  // profile page.
  const revokedMain = openAt('#/profiles/server/default', {
    serverProfile: REVOKED,
  })
  const revoked = await within(revokedMain).findByText(en('session-revoked'))
  expect(revoked.getAttribute('role'), 'the alert of S1.9 stays').toBe('alert')
  expect(revoked.firstChild, 'the icon is the paragraph FIRST child').toBe(
    revoked.children[0],
  )
  expectIcon(revoked, 'triangle-alert', 'the session-revoked Notice')
  expect(
    text(revoked),
    'the alert keeps its words, so the live region announces the same',
  ).toBe(en('session-revoked'))
  cleanup()

  // signin-insecure: the plain-HTTP page (I1).
  secure(false)
  const insecureMain = openAt('#/sign-in', { session: 'signed-out' })
  const insecure = await within(insecureMain).findByText(en('signin-insecure'))
  expect(insecure.classList.contains('notice'), 'the .notice look').toBe(true)
  expect(insecure.firstChild, 'the icon is the paragraph FIRST child').toBe(
    insecure.children[0],
  )
  expectIcon(insecure, 'lock-open', 'the signin-insecure Notice')
  expect(text(insecure), 'and its words').toBe(en('signin-insecure'))
})

test('H8b.3 model profile: the engine-unavailable Notice holds Icon triangle-alert first, and its words are unchanged', async () => {
  const main = openAt(PROFILE, {
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { engineAvailable: false })),
  })
  const words = en('model-profile-engine-unavailable', 'llama.cpp')
  const note = await within(main).findByText(words)
  expect(note.classList.contains('notice'), 'the .notice look').toBe(true)
  expect(note.firstChild, 'the icon is the paragraph FIRST child').toBe(
    note.children[0],
  )
  expectIcon(note, 'triangle-alert', 'the engine-unavailable Notice')
  expect(text(note), 'and its words, with the engine named').toBe(words)
})

// ── H8b.4: the filter field (DOM) ─────────────────────────────────────────

test('H8b.4: the filter row holds Icon search as its first child before the input, and the label, the hint, Escape and Clear are unchanged', async () => {
  const main = openAt('#/profiles/model')
  await within(main).findByRole('table', {
    name: en('model-profiles-title'),
  })
  const row = main.querySelector('.model-profiles-filter-row')
  expect(row, 'the row of the filter field').toBeTruthy()
  expectIcon(row!, 'search', 'the filter row')
  const glyph = row!.children[0]!
  const input = (await within(main).findByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  })) as HTMLInputElement
  expect(row!.contains(input), 'the field is in the row').toBe(true)
  expect(
    before(glyph, input),
    'the icon comes BEFORE the input, so the sheet can place it inside the field',
  ).toBe(true)
  expect(
    row!.querySelectorAll('svg.icon').length,
    'one icon in the row: the field has its own',
  ).toBe(1)
  // The field is named by its label and described by its hint, as 2b built it.
  expect(input.id, 'the id the label points at').toBe('model-profiles-filter')
  expect(input.getAttribute('aria-describedby'), 'the hint of 2b').toBe(
    'model-profiles-filter-hint',
  )
  const hint = main.querySelector('#model-profiles-filter-hint')
  expect(text(hint!), 'the hint is the string').toBe(
    en('model-profiles-filter-hint'),
  )

  // Clear and Escape keep working with the icon in the row, and the icon stays
  // the first child while Clear shows.
  fireEvent.change(input, { target: { value: 'gemma' } })
  const clear = await within(main).findByRole('button', {
    name: en('model-profiles-filter-clear'),
  })
  expect(
    [...row!.children].map((el) => el.tagName.toLowerCase()),
    'the row is the icon, the field, Clear',
  ).toEqual(['svg', 'input', 'button'])
  expect(row!.children[0], 'Clear does not push the icon off first place').toBe(
    glyph,
  )
  expect(before(glyph, clear), 'and the icon comes before Clear').toBe(true)
  fireEvent.keyDown(input, { key: 'Escape', code: 'Escape' })
  expect(input.value, 'Escape empties the field, as 2b').toBe('')
})

// ── H8b.6: the field error ────────────────────────────────────────────────

test('H8b.6: the field error draws Icon circle-alert in place of its own svg, keeps its text, its id and the aria-describedby link, and no .error-icon is left on the page', async () => {
  const main = openAt(PROFILE)
  await markTopK(main)
  const err = within(main).getByText(WHOLE)
  expect(err.tagName, 'a paragraph (128)').toBe('P')
  expect(err.id, 'the id 128 gives the field error').toBe(
    'model-profile-top-k-error',
  )
  expect(
    err.classList.contains('field-error'),
    'the class DESIGN 5.2 uses',
  ).toBe(true)
  expect(err.firstChild, 'the icon is the paragraph FIRST child').toBe(
    err.children[0],
  )
  expectIcon(err, 'circle-alert', 'the field error of 128')
  expect(
    text(err),
    'the words of 129 exactly: the icon of 24 adds no text',
  ).toBe(WHOLE)
  expect(
    [...main.querySelectorAll('.field-error')].map((el) => el.id),
    'this field is the only one with an error',
  ).toEqual(['model-profile-top-k-error'])
  expect(
    main.querySelector('svg.error-icon'),
    'the drawn svg of Field.tsx, with its circle and two rects, is gone',
  ).toBeNull()
  expect(
    main.querySelector('.error-icon'),
    'and its class is used by nothing on the page',
  ).toBeNull()
  // The link the announcement runs on (section 5: the field error's announcement).
  const control = main.querySelector(
    '[aria-describedby~="model-profile-top-k-error"]',
  )
  expect(control, 'a control names the error in aria-describedby').toBeTruthy()
  expect(
    control!.getAttribute('aria-describedby')!.split(/\s+/),
    'the ids it names, in order, include the error',
  ).toContain('model-profile-top-k-error')
  expect(
    control!.getAttribute('aria-invalid'),
    'and the control says it is invalid',
  ).toBe('true')
})

// ── H8b.7: the sweep over every page ──────────────────────────────────────

test.each(PAGES)(
  'H8b.7 $what: every svg of the page is aria-hidden and not focusable',
  async (page) => {
    const main = openAt(page.hash, page.options)
    await page.ready(main)
    await page.dirty?.(main)
    const svgs = [...document.querySelectorAll('svg')]
    expect(
      svgs.length,
      `${page.what}: the page draws at least the mark of the banner`,
    ).toBeGreaterThan(0)
    for (const svg of svgs) {
      const who = `${svg.getAttribute('class') ?? 'svg of no class'}`
      expect(
        svg.getAttribute('aria-hidden'),
        `${page.what}, ${who}: assistive technology reads the text beside it, not the drawing`,
      ).toBe('true')
      expect(
        svg.getAttribute('focusable'),
        `${page.what}, ${who}: it takes no place in the tab order (IE and legacy engines)`,
      ).toBe('false')
    }
  },
)

// ── H8b.8: no icon is alone ───────────────────────────────────────────────

test.each(PAGES)(
  'H8b.8 $what: every element that holds an .icon also holds text, the filter row excepted, whose field is named by its label',
  async (page) => {
    const main = openAt(page.hash, page.options)
    await page.ready(main)
    await page.dirty?.(main)
    const icons = [...document.querySelectorAll('.icon')]
    expect(
      icons.length,
      `${page.what}: the page holds the icons of the shell`,
    ).toBeGreaterThan(0)
    for (const icon of icons) {
      const host = icon.parentElement
      expect(host, 'an icon has a host').not.toBeNull()
      if (host!.classList.contains('model-profiles-filter-row')) {
        // The one exception the brief names: the field beside it is named.
        expect(
          within(main).getByRole('searchbox', {
            name: en('model-profiles-filter-label'),
          }),
          'the filter icon sits beside a field its label names (1.1.1)',
        ).toBeTruthy()
        continue
      }
      expect(
        text(host!),
        'no icon is alone: the element that holds it holds the text it sits beside (1.4.1: nothing reads only from an icon)',
      ).not.toBe('')
    }
  },
)
