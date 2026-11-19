// Hearth H7 (fe/briefs/08-hearth-h7-signin.md, section 1): the sign-in page
// carries a decorative drawing beside the form, and the form sits in a lifted
// panel. The names of the parts are the brief's: `signin-page` (the box),
// `signin-art` (the drawing, hidden), `signin-panel` (the box that holds the
// form). H7.4 to H7.7 are stylesheet text, pinned in HearthH7.test.ts.
//
// jsdom measures no layout, so what the brief claims of the geometry (the two
// columns at 1280, no sideways scroll at 320 and at 400 % zoom, the art no
// taller than the panel, the picture readable in dark, no art in forced
// colours) belongs to its section 2, the designer's UX pass. What jsdom pins
// of those claims is pinned here and in the stylesheet text: the art and the
// panel are the box's only two children in the brief's order, the drawing is
// hidden from assistive technology and says nothing, and nothing readable
// stands before the h1.
//
// The focus of H7.9 (the h1 takes it on arrival, the tab order) is behaviour
// H2b and 1a.13 already pin: `SignIn.test.tsx` holds, so H7.10 keeps it.
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
import { before, en, fakeBff, reply, startAt, stubDialog } from './test/bff.ts'

/** The three classes of the brief. */
const PAGE = 'signin-page'
const ART = 'signin-art'
const PANEL = 'signin-panel'

beforeEach(() => startAt('#/sign-in'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The sign-in `main`, once its h1 is there: the session read settles first,
 *  so right after the mount `main` holds only the loading `p`. */
async function mainOf(hash: string): Promise<HTMLElement> {
  startAt(hash)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  return main
}

/** H7.1: the page is the box of the art and the panel, in that order, the
 * form of today inside the panel and nothing of it in the art. */
function expectArtThenPanel(main: HTMLElement) {
  const art = main.querySelector(`.${ART}`)
  expect(art, 'the page shows a div.signin-art').toBeTruthy()
  const box = art!.parentElement
  expect(box, 'the art sits in the box the app shows').toBeTruthy()
  expect(
    box!.classList.contains(PAGE),
    'the box keeps the class signin-page',
  ).toBe(true)
  expect(main.contains(box), 'the box is inside main').toBe(true)
  const panel = box!.querySelector(`.${PANEL}`)
  expect(panel, 'the panel is the div.signin-panel').toBeTruthy()
  expect(
    Array.from(box!.children),
    'the art is the first child of the box, the panel the next one, and they are all of it',
  ).toEqual([art, panel])

  // the panel is the box that holds the form: the h1 is in it, never in the art
  expect(panel!.querySelector('h1'), 'the panel holds the h1').toBeTruthy()
  expect(art!.querySelector('h1'), 'the art holds no heading').toBeNull()
  // the art is decoration and the panel is not: only the art is hidden
  expect(art!.getAttribute('aria-hidden'), 'the art is aria-hidden').toBe(
    'true',
  )
  expect(
    panel!.getAttribute('aria-hidden'),
    'the panel is not hidden: the form is the page',
  ).toBeNull()

  // H7.1: nothing in the panel moved, the order of H2b holds
  const head = panel!.querySelector('h1')!
  const intro = panel!.querySelector('p')!
  const form = panel!.querySelector('form')!
  expect(intro.textContent, 'the intro is the first text of the panel').toBe(
    en('signin-intro'),
  )
  expect(before(head, intro), 'the h1, then the intro').toBe(true)
  expect(before(intro, form), 'the intro, then the form').toBe(true)
  const user = within(panel as HTMLElement).getByRole('textbox', {
    name: en('signin-username'),
  })
  const password = within(panel as HTMLElement).getByLabelText(
    en('signin-password'),
    { selector: 'input' },
  )
  const submit = within(panel as HTMLElement).getByRole('button', {
    name: en('signin-submit'),
  })
  expect(before(user, password), 'the user name, then the password').toBe(true)
  expect(before(password, submit), 'the password, then the button').toBe(true)
}

// ── H7.1: the page is the art and the panel, the dialog is not ─────────────

test('H7.1: at #/sign-in the page is the art and the panel, in that order, the form inside the panel', async () => {
  fakeBff({ session: 'signed-out' })
  const main = await mainOf('#/sign-in')
  expectArtThenPanel(main)
  // the page of H2b, unchanged: it is not a protected page
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
})

test('H7.1: in place of a protected page, the same art and the same panel (#/profiles/model, signed out)', async () => {
  fakeBff({ session: 'signed-out' })
  const main = await mainOf('#/profiles/model')
  expectArtThenPanel(main)
  // the page keeps its notice that signing in is needed (1a.10)
  expect(within(main).getByText(en('signin-required'))).toBeTruthy()
})

test('H7.1 guard: the sign-in dialog a save bar opens is exactly what it is today: no page, no art, no panel', async () => {
  const dialogs = stubDialog()
  try {
    startAt('#/instances/default')
    const bff = fakeBff({
      save: () => reply(401, { key: 'session-required', args: [] }),
    })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('heading', {
      level: 3,
      name: en('server-network'),
    })
    const port = within(
      within(main).getByRole('group', { name: 'Port' }),
    ).getByRole('textbox', { name: 'Port' }) as HTMLInputElement
    fireEvent.change(port, { target: { value: '8581' } })
    fireEvent.click(
      within(main).getByRole('button', { name: en('save-button') }),
    )
    await waitFor(() =>
      expect(bff.to('PUT', '/api/instances/default/server')).toHaveLength(1),
    )
    fireEvent.click(
      await within(main).findByRole('button', { name: en('session-sign-in') }),
    )
    const dialog = screen.getByRole('dialog', { name: en('signin-title') })
    expect(dialogs.opened).toContain(dialog)
    for (const cls of [PAGE, ART, PANEL]) {
      expect(
        dialog.querySelector(`.${cls}`),
        `no .${cls} in the dialog`,
      ).toBeNull()
      expect(
        document.querySelector(`.${cls}`),
        `no .${cls} anywhere`,
      ).toBeNull()
    }
    // the dialog keeps today's form, with its own title from the Dialog
    within(dialog).getByRole('textbox', { name: en('signin-username') })
    within(dialog).getByRole('button', { name: en('signin-submit') })
  } finally {
    dialogs.restore()
  }
})

// ── H7.2: the drawing is one svg, hidden and silent ────────────────────────

test('H7.2: the art holds exactly one svg with its viewBox, hidden, with no size attribute and no text in it', async () => {
  fakeBff({ session: 'signed-out' })
  const main = await mainOf('#/sign-in')
  const art = main.querySelector(`.${ART}`)
  expect(art).toBeTruthy()
  expect(art!.children.length, 'the art holds one element: the drawing').toBe(1)
  const svg = art!.firstElementChild!
  expect(svg.tagName.toLowerCase()).toBe('svg')
  expect(svg.getAttribute('viewBox')).toBe('0 0 420 260')
  expect(svg.getAttribute('focusable')).toBe('false')
  expect(svg.getAttribute('aria-hidden')).toBe('true')
  // CSS gives the size (H7.6), so no attribute fights it
  expect(svg.hasAttribute('width'), 'no width attribute').toBe(false)
  expect(svg.hasAttribute('height'), 'no height attribute').toBe(false)
  // decoration says nothing: no text of any kind, and no name for it
  for (const tag of ['text', 'title', 'desc', 'foreignObject', 'a'])
    expect(svg.querySelector(tag), `no <${tag}> in the drawing`).toBeNull()
  expect(svg.textContent, 'the drawing holds no text node').toBe('')
})

// ── H7.3: the drawing, as the brief writes it ──────────────────────────────

/** The children of the svg, in the order and with the attributes of H7.3. */
const DRAWING: Array<{ tag: string; attrs: Record<string, string> }> = [
  { tag: 'circle', attrs: { class: 'art-sun', cx: '330', cy: '70', r: '38' } },
  { tag: 'path', attrs: { class: 'art-ground', d: 'M0 214h420v46H0z' } },
  {
    tag: 'path',
    attrs: {
      class: 'art-door',
      d: 'M120 214V112a90 90 0 0 1 180 0v102z',
      'stroke-width': '3',
    },
  },
  {
    tag: 'rect',
    attrs: {
      class: 'art-panel',
      x: '160',
      y: '120',
      width: '100',
      height: '80',
      rx: '12',
      'stroke-width': '3',
    },
  },
  {
    tag: 'rect',
    attrs: {
      class: 'art-bar-on',
      x: '174',
      y: '136',
      width: '72',
      height: '12',
      rx: '6',
    },
  },
  {
    tag: 'rect',
    attrs: {
      class: 'art-bar',
      x: '174',
      y: '156',
      width: '72',
      height: '12',
      rx: '6',
      'stroke-width': '2.5',
    },
  },
  {
    tag: 'rect',
    attrs: {
      class: 'art-bar',
      x: '174',
      y: '176',
      width: '72',
      height: '12',
      rx: '6',
      'stroke-width': '2.5',
    },
  },
  { tag: 'circle', attrs: { class: 'art-knob', cx: '236', cy: '142', r: '3' } },
  {
    tag: 'path',
    attrs: {
      class: 'art-cable',
      d: 'M260 196c40 0 40 18 80 18',
      'stroke-width': '3',
      'stroke-linecap': 'round',
      'stroke-dasharray': '1 8',
    },
  },
  {
    tag: 'rect',
    attrs: {
      class: 'art-panel',
      x: '320',
      y: '180',
      width: '64',
      height: '34',
      rx: '6',
      'stroke-width': '3',
    },
  },
  {
    tag: 'path',
    attrs: {
      class: 'art-line',
      d: 'M312 214h80',
      'stroke-width': '3',
      'stroke-linecap': 'round',
    },
  },
  {
    tag: 'path',
    attrs: {
      class: 'art-stem',
      d: 'M60 214c0-30 10-46 26-58M86 214c0-22-6-38-20-50',
      'stroke-width': '3',
      'stroke-linecap': 'round',
    },
  },
  {
    tag: 'ellipse',
    attrs: {
      class: 'art-leaf',
      cx: '86',
      cy: '152',
      rx: '12',
      ry: '6',
      transform: 'rotate(-35 86 152)',
    },
  },
  {
    tag: 'ellipse',
    attrs: {
      class: 'art-leaf',
      cx: '64',
      cy: '162',
      rx: '11',
      ry: '5',
      transform: 'rotate(30 64 162)',
    },
  },
]

/** The shape an element draws: its tag, then each attribute of the brief. */
function shapeOf(el: Element, attrs: Record<string, string>): string {
  return [
    el.tagName.toLowerCase(),
    ...Object.keys(attrs).map((name) => `${name}="${el.getAttribute(name)}"`),
  ].join(' ')
}

/** The same line, as the brief writes it. */
function shapeWanted(shape: { tag: string; attrs: Record<string, string> }) {
  return [
    shape.tag,
    ...Object.entries(shape.attrs).map(([name, v]) => `${name}="${v}"`),
  ].join(' ')
}

test('H7.3: the svg holds the fourteen shapes of the brief, in order, each with its class and geometry', async () => {
  fakeBff({ session: 'signed-out' })
  const main = await mainOf('#/sign-in')
  const svg = main.querySelector(`.${ART} svg`)
  expect(svg).toBeTruthy()
  const kids = Array.from(svg!.children)
  expect(kids.length, 'the brief lists fourteen shapes').toBe(DRAWING.length)
  expect(
    kids.map((el, n) => shapeOf(el, DRAWING[n]!.attrs)),
    'the tag, the class and the geometry of each shape, in the order of the brief',
  ).toEqual(DRAWING.map(shapeWanted))
  // section 0: the colour comes from the class, and the CSS gives it. Not one
  // engine resolves fill="var(--x)" written as an attribute, so the drawing
  // carries geometry only, and no fill or stroke of its own.
  for (const el of [svg as Element, ...kids]) {
    expect(el.getAttribute('fill'), 'no fill attribute').toBeNull()
    expect(el.getAttribute('stroke'), 'no stroke attribute').toBeNull()
  }
})

// ── H7.9: the reading order a screen reader meets ──────────────────────────

test('H7.9: inside main nothing readable comes before the h1, and the tab order is the one of today', async () => {
  fakeBff({ session: 'signed-out' })
  const main = await mainOf('#/sign-in')
  const heading = within(main).getByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  const art = main.querySelector(`.${ART}`)
  expect(art, 'the page shows the art').toBeTruthy()
  expect(
    art!.getAttribute('aria-hidden'),
    'the art is hidden, so a screen reader passes it',
  ).toBe('true')

  // 1.3.2: every element of main that comes before the h1 is inside the
  // hidden art, or says nothing itself. An element that carries text there is
  // a thing a screen reader meets before the page names itself. The box and
  // the panel (H7.1) hold the h1: an element whose subtree contains the
  // heading draws no text before it, its text is the heading and what
  // follows, so it is not "before" it; compareDocumentPosition counts a
  // descendant as following its ancestor, so the walk skips those.
  const off: string[] = []
  for (const el of Array.from(main.querySelectorAll('*'))) {
    if (el.contains(heading)) continue
    if (!before(el, heading)) continue
    if (art!.contains(el) || (el.textContent ?? '').trim() === '') continue
    off.push(`${el.tagName.toLowerCase()}${el.getAttribute('class') ?? ''}`)
  }
  expect(off, 'nothing readable before the h1').toEqual([])

  // 2.4.3: the order of the things a Tab reaches. The art is not one of them,
  // so it must not come between them. 09 S1a.4: signed out there is no nav, so
  // the skip link is followed by the user name (the order of S1a.5).
  const skip = screen.getByRole('link', { name: en('skip-to-content') })
  expect(
    screen.queryByRole('navigation', { name: en('nav-label') }),
    '09 S1a.4: signed out there is no navigation landmark',
  ).toBeNull()
  const user = within(main).getByRole('textbox', {
    name: en('signin-username'),
  })
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  })
  const submit = within(main).getByRole('button', { name: en('signin-submit') })
  const order: Array<[Element, Element, string]> = [
    [skip, user, 'the skip link, then the user name'],
    [user, password, 'the user name, then the password'],
    [password, submit, 'the password, then the button'],
  ]
  for (const [a, b, what] of order) expect(before(a, b), what).toBe(true)
})
