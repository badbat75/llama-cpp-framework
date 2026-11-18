// The save bar's alert of a form that becomes clean again (fe/briefs/
// 02h-model-profile-save.md, sub-slice 2h.4b, criteria 198b to 198h): each
// alert the brief lists (198b) goes as soon as the form has no edits left, on
// both pages that carry a save bar (the model profile page and the Server
// page), and does not come back when the form turns dirty again; the session's
// alert and the 409 alert stay with their button (198c); the form is made
// clean by every route 198d names (198d); a field the user never touched keeps
// its mark (198e); the bar goes quiet, not loud (198f); and an answer that
// lands on a form that is already clean sets what its kind sets and nothing
// else (198g). The rows are the scenarios of 198h. The bar, the PUT and the
// older answers of a save are 01's 30 to 37 and 2h's 135 to 149, in
// ServerSave.test.tsx and ModelProfileSave1 to 6.test.tsx; the fixtures and
// checks are in test/savealert.tsx.
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { deferred, en, reply, stubDialog, type BffOptions } from './test/bff.ts'
import {
  AS_LOADED,
  BIND,
  BIND_LOCALHOST,
  CTX,
  ID,
  LOG_LEVEL,
  MODE,
  POOLING,
  PORT,
  PORT_LOADED,
  PROFILE_GET,
  PROFILE_PUT,
  RELOAD,
  SERVER_GET,
  SERVER_PUT,
  SIGN_IN,
  WEBUI,
  WEBUI_TOGGLE,
  box,
  clickMoves,
  expectAction,
  expectAlert,
  expectClean,
  expectDirty,
  expectError,
  expectNoAlert,
  expectNoError,
  expectNotAnnounced,
  expectQuiet,
  fitBox,
  focusable,
  marked,
  pick,
  pressAction,
  profileLoaded,
  profileStatus,
  saveButton,
  selectMoves,
  selectOf,
  sent,
  serverLoaded,
  serverStatus,
  textMoves,
  typeInput,
  type Bff,
  type Moves,
  type Page,
} from './test/savealert.tsx'

/** The edit of the model profile rows: Embedding mode off, so picking it back
 * onto its loaded option makes the form clean. */
const MODE_DIRTY = 'false'
/** The edit of the Server rows: Port typed to this, the loaded number typed
 * back makes the form clean. */
const PORT_DIRTY = '8081'
/** The 422 of 198h's Pooling row (142), and one on a field with nothing to
 * focus, answered `save-refused-at` (198a). */
const POOLING_ARGS = ['pooling', 'mean', 'default, none, mean, cls, last, rank']
const POOLING_REFUSAL = { key: 'not-one-of', args: POOLING_ARGS }
const POOLING_TEXT = en('api-not-one-of', ...POOLING_ARGS)
const MOE = en('model-profile-moe-cpu-layers')
const HW_CARD = en('model-profile-hardware-config')
const MOE_REFUSAL = { key: 'not-a-number', args: ['n-cpu-moe', 'many'] }
const MOE_TEXT = en('api-not-a-number', 'n-cpu-moe', 'many')
/** 198h's Server row: a refused comment marker in ModelsDir (144). */
const FOLDER = en('server-models-folder')
const MARKERS_TEXT = en('api-comment-markers', 'ModelsDir')
/** The refused session of the model profile page (1c.43). */
const REVOKED = en('session-revoked')
const ENDED = en('session-ended')
const FAILED = en('save-failed')
const TIMEOUT = en('save-timeout')
const UNAVAILABLE = en('api-client-unavailable')
const NOT_AUTHORIZED = en('api-client-not-authorized')
const INVALID = en('save-invalid')

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
})

/** The model profile page, with Embedding mode off. */
async function profileDirty(
  save?: BffOptions['modelProfileSave'],
  over: Record<string, string> = {},
): Promise<Page> {
  const page = await profileLoaded(save, over)
  pick(page.main, MODE, MODE_DIRTY)
  return page
}

/** The Server page, with Port typed to `typed`. */
async function serverDirty(
  typed: string,
  save?: BffOptions['save'],
): Promise<Page> {
  const page = await serverLoaded(save)
  typeInput(box(page.main, PORT), typed)
  return page
}

/** Port typed back to its loaded number: the Server form is clean. */
function portBack(main: HTMLElement) {
  typeInput(box(main, PORT), PORT_LOADED)
}

// ── 198b, 198e: every alert of the brief, on the page that shows it ───────

/** One row of 198h's list: the kind of alert, the control the user edits (with
 * its loaded value and the value typed into it), how the save answered (none:
 * a client-check row, which sends nothing), the alert's text, the values the
 * page reads with, and the field the answer marks (`keeps`: a field the user
 * never touched, so 198e says it keeps its mark once the alert is gone; not
 * `keeps`: the field the user typed back). */
type Row = {
  what: string
  control: { label: string; kind: 'box' | 'select' }
  from: string
  to: string
  loaded?: Record<string, string>
  answer?: () => Response
  alert: string
  mark?: {
    label: string
    kind: 'box' | 'select'
    text: string
    keeps: boolean
  }
}

/** Sets the row's control to `value`. */
function set(main: HTMLElement, row: Row, value: string) {
  if (row.control.kind === 'select') pick(main, row.control.label, value)
  else typeInput(box(main, row.control.label), value)
}

/** The body of a row of 198b, on either page. The last check is the one this
 * slice adds, so a red names it. */
async function cleanRow(
  main: HTMLElement,
  bff: Bff,
  status: HTMLElement,
  put: string,
  row: Row,
) {
  set(main, row, row.to)
  if (row.answer === undefined) {
    fireEvent.click(saveButton(main))
    await within(main).findByText(row.alert)
    expect(bff.to('PUT', put), 'a client check sends nothing').toHaveLength(0)
  } else {
    await sent(main, bff, put)
    await within(main).findByText(row.alert)
  }
  expectAlert(status, row.alert, row.what)
  expectDirty(main, status, row.what)
  if (row.mark !== undefined) {
    expectError(
      marked(main, row.mark.label, row.mark.kind),
      row.mark.text,
      row.what,
    )
  }
  // The one edit back: the form is clean (30a).
  set(main, row, row.from)
  expectClean(main, status, row.what)
  if (row.mark !== undefined) {
    const el = marked(main, row.mark.label, row.mark.kind)
    if (row.mark.keeps) {
      expectError(el, row.mark.text, `${row.what}: the untouched field`)
    } else {
      expectNoError(el, `${row.what}: the field typed back`)
    }
  }
  expectNoAlert(status, row.what)
  // The alert does not come back when the form turns dirty again (198b).
  set(main, row, row.to)
  expectDirty(main, status, `${row.what}: dirty again`)
  expectNoAlert(status, `${row.what}: dirty again`)
  set(main, row, row.from)
  expectClean(main, status, `${row.what}: clean again`)
  expectNoAlert(status, `${row.what}: clean again`)
}

const serverRow = (
  what: string,
  answer: (() => Response) | undefined,
  alert: string,
  mark?: Row['mark'],
): Row => ({
  what,
  control: { label: PORT, kind: 'box' },
  from: PORT_LOADED,
  to: PORT_DIRTY,
  answer,
  alert,
  mark,
})

const SERVER_ROWS: Row[] = [
  {
    // 198h's Server scenario: the client check of Port.
    what: '198h: the client check of Port abc (save-invalid)',
    control: { label: PORT, kind: 'box' },
    from: PORT_LOADED,
    to: 'abc',
    alert: INVALID,
    mark: {
      label: PORT,
      kind: 'box',
      text: en('field-error-number', '1', '65535'),
      keeps: false,
    },
  },
  {
    // 198h's Server scenario: the refused comment marker in ModelsDir, a field
    // nobody edited (144).
    what: '198h: a refused comment marker in ModelsDir (save-refused)',
    control: { label: PORT, kind: 'box' },
    from: PORT_LOADED,
    to: PORT_DIRTY,
    answer: () =>
      reply(422, {
        key: 'comment-markers',
        args: ['ModelsDir'],
        field: 'ModelsDir',
      }),
    alert: en('save-refused', MARKERS_TEXT),
    mark: { label: FOLDER, kind: 'box', text: MARKERS_TEXT, keeps: true },
  },
  {
    // 198h's Server scenario: the 503, cleaned by Port typed back.
    what: '198h: a 503 client-unavailable (api-client-unavailable)',
    control: { label: PORT, kind: 'box' },
    from: PORT_LOADED,
    to: PORT_DIRTY,
    answer: () => reply(503, { key: 'client-unavailable', args: [] }),
    alert: UNAVAILABLE,
  },
  serverRow('a 500 (save-failed)', () => reply(500, {}), FAILED),
  serverRow(
    'the service time-out (save-timeout)',
    () => reply(504, { key: 'client-timeout', args: [] }),
    TIMEOUT,
  ),
  serverRow(
    'a 403 client-not-authorized (api-client-not-authorized)',
    () => reply(403, { key: 'client-not-authorized', args: [] }),
    NOT_AUTHORIZED,
  ),
  serverRow(
    // 01's 37a.
    'a 404 client-no-instance (instance-not-found)',
    () => reply(404, { key: 'client-no-instance', args: ['default'] }),
    en('instance-not-found', 'default'),
  ),
]

test.each(SERVER_ROWS)(
  '198b, the Server page: $what goes when the form is clean again, does not come back, and a field nobody touched keeps its mark',
  async (row) => {
    const { bff, main } = await serverLoaded(row.answer)
    await cleanRow(main, bff, serverStatus(main), SERVER_PUT, row)
  },
)

const profileRow = (
  what: string,
  answer: (() => Response) | undefined,
  alert: string,
  mark?: Row['mark'],
): Row => ({
  what,
  control: { label: MODE, kind: 'select' },
  from: AS_LOADED['embeddings'],
  to: MODE_DIRTY,
  answer,
  alert,
  mark,
})

const PROFILE_ROWS: Row[] = [
  {
    // 198h's model profile scenario: ctx-size abc loaded and untouched (162),
    // Embedding mode the edit. The client check marks the field nobody edited.
    what: '198h: the client check of a loaded ctx-size abc (save-invalid)',
    control: { label: MODE, kind: 'select' },
    from: AS_LOADED['embeddings'],
    to: MODE_DIRTY,
    loaded: { 'ctx-size': 'abc' },
    alert: INVALID,
    mark: {
      label: CTX,
      kind: 'box',
      text: en('field-error-number-min', '0'),
      keeps: true,
    },
  },
  {
    // 198h's model profile scenario: the 422 on Pooling, cleaned by picking
    // Pooling's loaded option, the extra option of 132a (142).
    what: '198h: a 422 not-one-of on pooling (save-refused)',
    control: { label: POOLING, kind: 'select' },
    from: AS_LOADED['pooling'],
    to: 'mean',
    answer: () => reply(422, POOLING_REFUSAL),
    alert: en('save-refused', POOLING_TEXT),
    mark: {
      label: POOLING,
      kind: 'select',
      text: POOLING_TEXT,
      keeps: false,
    },
  },
  {
    // 198a: a refusal on a field with nothing to focus, marked where it is.
    what: 'a 422 on a field with nothing to focus (save-refused-at)',
    control: { label: MODE, kind: 'select' },
    from: AS_LOADED['embeddings'],
    to: MODE_DIRTY,
    answer: () => reply(422, MOE_REFUSAL),
    alert: en('save-refused-at', MOE_TEXT, MOE, HW_CARD),
    mark: { label: MOE, kind: 'box', text: MOE_TEXT, keeps: true },
  },
  {
    // 198h's model profile scenario: the 404, cleaned by the edit put back.
    what: '198h: a 404 no-model-profile-named (api-no-model-profile-named)',
    control: { label: MODE, kind: 'select' },
    from: AS_LOADED['embeddings'],
    to: MODE_DIRTY,
    answer: () => reply(404, { key: 'no-model-profile-named', args: [] }),
    alert: en('api-no-model-profile-named', ID),
  },
  profileRow('a 500 (save-failed)', () => reply(500, {}), FAILED),
  profileRow(
    'the service time-out (save-timeout)',
    () => reply(504, { key: 'client-timeout', args: [] }),
    TIMEOUT,
  ),
  profileRow(
    'a 403 client-not-authorized (api-client-not-authorized)',
    () => reply(403, { key: 'client-not-authorized', args: [] }),
    NOT_AUTHORIZED,
  ),
  profileRow(
    'a 503 client-unavailable (api-client-unavailable)',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    UNAVAILABLE,
  ),
]

test.each(PROFILE_ROWS)(
  '198b, the model profile page: $what goes when the form is clean again, does not come back, and a field nobody touched keeps its mark',
  async (row) => {
    const { bff, main } = await profileLoaded(row.answer, row.loaded)
    await cleanRow(main, bff, profileStatus(main), PROFILE_PUT, row)
  },
)

// ── 198c: the alerts that stay ────────────────────────────────────────────

/** 198c on one page: a 409, the form made clean, the alert and Reload stay,
 * and Reload of the clean form reads at once, with no dialog (145, 01's 35). */
async function fortyNineStays(
  page: Promise<Page>,
  statusOf: (main: HTMLElement) => HTMLElement,
  get: string,
  put: string,
  text: string,
  makeClean: (main: HTMLElement) => void,
) {
  const { bff, main } = await page
  const status = statusOf(main)
  await sent(main, bff, put)
  await within(main).findByText(text)
  expectAlert(status, text, 'the 409')
  makeClean(main)
  expectClean(main, status, 'the clean form')
  // A clean form still holds the stale revision: the alert and its Reload stay.
  expectAlert(status, text, 'the 409 of a clean form')
  expectAction(status, RELOAD, 'the 409 of a clean form')
  pressAction(status, RELOAD)
  await waitFor(() =>
    expect(bff.to('GET', get).length, 'the clean form reads at once').toBe(2),
  )
  expect(dialogs.opened, 'no dialog on a clean form').toHaveLength(0)
  expect(screen.queryByRole('dialog'), 'no dialog').toBeNull()
}

test('198c, the Server page: a 409 stays with Reload when the form is clean, and the clean form Reloads at once with no dialog', async () => {
  await fortyNineStays(
    serverDirty(PORT_DIRTY, () =>
      reply(409, { key: 'server-changed-elsewhere', args: [] }),
    ),
    serverStatus,
    SERVER_GET,
    SERVER_PUT,
    en('api-server-changed-elsewhere'),
    portBack,
  )
})

test('198c, the model profile page: a 409 stays with Reload when the form is clean, and the clean form Reloads at once with no dialog', async () => {
  await fortyNineStays(
    profileDirty(() =>
      reply(409, { key: 'model-profile-changed-elsewhere', args: [] }),
    ),
    profileStatus,
    PROFILE_GET,
    PROFILE_PUT,
    en('api-model-profile-changed-elsewhere'),
    (main) => pick(main, MODE, AS_LOADED['embeddings']),
  )
})

test('198c, the Server page: a 401 keeps session-ended and its Sign in when the form is clean', async () => {
  const { bff, main } = await serverDirty(PORT_DIRTY, () =>
    reply(401, { key: 'session-required', args: [] }),
  )
  const status = serverStatus(main)
  await sent(main, bff, SERVER_PUT)
  await within(main).findByText(ENDED)
  expectAlert(status, ENDED, 'the 401')
  portBack(main)
  expectClean(main, status, 'the clean form')
  expectAlert(status, ENDED, 'the 401 of a clean form')
  expectAction(status, SIGN_IN, 'the 401 of a clean form')
})

test('198c, the model profile page: a refused session keeps session-revoked and its Sign in when the form is clean', async () => {
  const { bff, main } = await profileDirty(() =>
    reply(403, { key: 'session-not-administrator', args: [] }),
  )
  const status = profileStatus(main)
  await sent(main, bff, PROFILE_PUT)
  await within(main).findByText(REVOKED)
  expectAlert(status, REVOKED, 'the refused session')
  pick(main, MODE, AS_LOADED['embeddings'])
  expectClean(main, status, 'the clean form')
  expectAlert(status, REVOKED, 'the refused session of a clean form')
  expectAction(status, SIGN_IN, 'the refused session of a clean form')
})

// ── 198d: clean by any route ──────────────────────────────────────────────

// ── 198d, the user's decision of 2026-11-18: Discard is one more route to a
// clean form, under the same rule as typing back: an alert with an answer
// button stays through Discard (the 409 with Reload, the session ones with
// Sign in), an alert without one goes. This replaces the earlier "Discard
// clears every alert" (136, 198d).

/** The bar's Discard. */
function discardButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-discard'),
  }) as HTMLButtonElement
}

/** The page's h1: the page hands it the focus when a clean Reload reads (198c). */
function pageH1(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name })
}

/** A 409, then Discard (both pages): the fields go back to the loaded values,
 * the form is clean and focus stays on Discard (136), and the alert stays
 * with its Reload (198b, 198d). Then Reload on that clean form: one read at
 * once, no dialog, the fresh read ends the alert and the focus goes to the
 * page's `h1` (198c). The last checks are the ones this slice adds. */
async function discardKeepsFortyNine(
  page: Promise<Page>,
  statusOf: (main: HTMLElement) => HTMLElement,
  get: string,
  put: string,
  text: string,
  h1: string,
  checkBack: (main: HTMLElement) => void,
) {
  const { bff, main } = await page
  const status = statusOf(main)
  await sent(main, bff, put)
  await within(main).findByText(text)
  expectAlert(status, text, 'the 409')
  const discard = discardButton(main)
  discard.focus()
  fireEvent.click(discard)
  checkBack(main)
  expectClean(main, status, 'the form Discard left')
  expect(document.activeElement, 'focus on Discard').toBe(discard)
  // An alert with an answer button stays through Discard (198d).
  expectAlert(status, text, 'the 409 of the form Discard left')
  expectAction(status, RELOAD, 'the 409 of the form Discard left')
  // Reload on the clean form reads at once, with no dialog (145, 01's 35).
  pressAction(status, RELOAD)
  expect(screen.queryByRole('dialog'), 'no dialog on a clean form').toBeNull()
  expect(dialogs.opened, 'no dialog opened').toHaveLength(0)
  await waitFor(() =>
    expect(bff.to('GET', get).length, 'the clean form reads at once').toBe(2),
  )
  // The read remounts the form (145), so the bar captured before Reload is
  // detached and keeps its old alert: re-query it from the live main, as the
  // next line re-queries the h1.
  await waitFor(() =>
    expectNoAlert(statusOf(main), 'the fresh read ends the 409'),
  )
  await waitFor(() =>
    expect(document.activeElement, 'focus on the page h1').toBe(
      pageH1(main, h1),
    ),
  )
}

test('198d, the Server page: a 409 then Discard: Port back to loaded, the alert and Reload stay, and Reload reads at once with no dialog, the alert gone and the focus on the h1', async () => {
  await discardKeepsFortyNine(
    serverDirty(PORT_DIRTY, () =>
      reply(409, { key: 'server-changed-elsewhere', args: [] }),
    ),
    serverStatus,
    SERVER_GET,
    SERVER_PUT,
    en('api-server-changed-elsewhere'),
    'default',
    (main) =>
      expect(box(main, PORT).value, 'Discard: Port back to loaded').toBe(
        PORT_LOADED,
      ),
  )
})

test('198d, the model profile page: a 409 then Discard: Embedding mode back to loaded, the alert and Reload stay, and Reload reads at once with no dialog, the alert gone and the focus on the h1', async () => {
  await discardKeepsFortyNine(
    profileDirty(() =>
      reply(409, { key: 'model-profile-changed-elsewhere', args: [] }),
    ),
    profileStatus,
    PROFILE_GET,
    PROFILE_PUT,
    en('api-model-profile-changed-elsewhere'),
    ID,
    (main) =>
      expect(selectOf(main, MODE).value, 'Discard: the mode back').toBe(
        AS_LOADED['embeddings'],
      ),
  )
})

/** The session ended (401), then Discard (both pages): the fields go back, the
 * form is clean and focus stays on Discard, and session-ended stays with its
 * Sign in (198d). */
async function discardKeepsSignIn(
  page: Promise<Page>,
  statusOf: (main: HTMLElement) => HTMLElement,
  put: string,
  checkBack: (main: HTMLElement) => void,
) {
  const { bff, main } = await page
  const status = statusOf(main)
  await sent(main, bff, put)
  await within(main).findByText(ENDED)
  expectAlert(status, ENDED, 'the 401')
  const discard = discardButton(main)
  discard.focus()
  fireEvent.click(discard)
  checkBack(main)
  expectClean(main, status, 'the form Discard left')
  expect(document.activeElement, 'focus on Discard').toBe(discard)
  // The session alert keeps its button through Discard (198d).
  expectAlert(status, ENDED, 'the 401 of the form Discard left')
  expectAction(status, SIGN_IN, 'the 401 of the form Discard left')
}

test('198d, the Server page: a 401 then Discard: Port back to loaded and the form clean, session-ended and Sign in stay', async () => {
  await discardKeepsSignIn(
    serverDirty(PORT_DIRTY, () =>
      reply(401, { key: 'session-required', args: [] }),
    ),
    serverStatus,
    SERVER_PUT,
    (main) =>
      expect(box(main, PORT).value, 'Discard: Port back to loaded').toBe(
        PORT_LOADED,
      ),
  )
})

test('198d, the model profile page: a 401 then Discard: Embedding mode back to loaded and the form clean, session-ended and Sign in stay', async () => {
  await discardKeepsSignIn(
    profileDirty(() => reply(401, { key: 'session-required', args: [] })),
    profileStatus,
    PROFILE_PUT,
    (main) =>
      expect(selectOf(main, MODE).value, 'Discard: the mode back').toBe(
        AS_LOADED['embeddings'],
      ),
  )
})

/** The guard of the new rule: an alert without an answer button still goes
 * with Discard, the form is clean and focus stays on Discard (136, 198b, 198d). */
async function discardGoes(
  page: Promise<Page>,
  statusOf: (main: HTMLElement) => HTMLElement,
  put: string,
) {
  const { bff, main } = await page
  const status = statusOf(main)
  await sent(main, bff, put)
  await within(main).findByText(FAILED)
  expectAlert(status, FAILED, 'the 500')
  const discard = discardButton(main)
  discard.focus()
  fireEvent.click(discard)
  expectClean(main, status, 'the form Discard left')
  expect(document.activeElement, 'focus on Discard').toBe(discard)
  // An alert without an answer button goes (198b).
  expectNoAlert(status, 'the 500 of the form Discard left')
}

test('198d, the Server page: a 500 then Discard: the alert goes, the form is clean, focus on Discard', async () => {
  await discardGoes(
    serverDirty(PORT_DIRTY, () => reply(500, {})),
    serverStatus,
    SERVER_PUT,
  )
})

test('198d, the model profile page: a 500 then Discard: the alert goes, the form is clean, focus on Discard', async () => {
  await discardGoes(
    profileDirty(() => reply(500, {})),
    profileStatus,
    PROFILE_PUT,
  )
})

/** One route of 198d, on a page of its own: `route` and `other` are armed
 * while the form is loaded; the form is made dirty by both, the save answers
 * 500, undoing `other` leaves one edit (the alert stays), undoing `route`
 * leaves none (the alert goes). */
async function routeGoes(
  open: () => Promise<Page>,
  put: string,
  statusOf: (main: HTMLElement) => HTMLElement,
  route: (main: HTMLElement) => Moves,
  other: (main: HTMLElement) => Moves,
  what: string,
) {
  // Each route opens a page of its own, so a test that runs it twice does
  // not stack two <main> (as cleanAnswers does).
  cleanup()
  const { bff, main } = await open()
  const status = statusOf(main)
  const r = route(main)
  const o = other(main)
  r.dirty(main)
  o.dirty(main)
  await sent(main, bff, put)
  await within(main).findByText(FAILED)
  expectAlert(status, FAILED, `${what}: after the 500`)
  // One edit left: the alert stays (198b).
  o.clean(main)
  expectDirty(main, status, `${what}: one edit left`)
  expectAlert(status, FAILED, `${what}: one edit left`)
  // The route makes the form clean: the alert goes (198d).
  r.clean(main)
  expectClean(main, status, `${what}: clean`)
  expectNoAlert(status, `${what}: clean`)
}

const portText = textMoves(PORT, PORT_DIRTY)
const modeSelect = selectMoves(MODE, MODE_DIRTY)
const webuiBox = clickMoves((main) =>
  within(within(main).getByRole('group', { name: WEBUI })).getByRole(
    'checkbox',
    { name: WEBUI_TOGGLE },
  ),
)
const bindRow = clickMoves((main) =>
  within(within(main).getByRole('group', { name: BIND })).getByRole(
    'checkbox',
    { name: BIND_LOCALHOST },
  ),
)
const fitCheckbox = clickMoves(fitBox)

test('198d, the Server page: the alert goes when a text is typed back exactly, and when a checkbox is re-ticked, and stays while an edit is left', async () => {
  await routeGoes(
    () => serverLoaded(() => reply(500, {})),
    SERVER_PUT,
    serverStatus,
    portText,
    webuiBox,
    'the text typed back',
  )
  await routeGoes(
    () => serverLoaded(() => reply(500, {})),
    SERVER_PUT,
    serverStatus,
    webuiBox,
    portText,
    'the checkbox re-ticked',
  )
})

test('198d, the Server page: the alert goes with a select back on its loaded option, and with Bind to rows back as loaded, and stays while an edit is left', async () => {
  await routeGoes(
    () => serverLoaded(() => reply(500, {})),
    SERVER_PUT,
    serverStatus,
    // The Log level select off its loaded option, onto the one below it.
    selectMoves(LOG_LEVEL, '2'),
    portText,
    "the select's loaded option",
  )
  await routeGoes(
    () => serverLoaded(() => reply(500, {})),
    SERVER_PUT,
    serverStatus,
    bindRow,
    portText,
    'the Bind to row back',
  )
})

test('198d, the model profile page: the alert goes when a text is typed back exactly, and when Fit to VRAM is re-ticked, and stays while an edit is left', async () => {
  await routeGoes(
    () => profileLoaded(() => reply(500, {})),
    PROFILE_PUT,
    profileStatus,
    textMoves(CTX, '4096'),
    modeSelect,
    'the text typed back',
  )
  await routeGoes(
    () => profileLoaded(() => reply(500, {})),
    PROFILE_PUT,
    profileStatus,
    fitCheckbox,
    modeSelect,
    'Fit to VRAM re-ticked',
  )
})

test('198d, the model profile page: the alert goes with a select back on its loaded option, the extra option of 132a included, and stays while an edit is left', async () => {
  // Pooling off the loaded value that is not on the list, back onto it.
  await routeGoes(
    () => profileLoaded(() => reply(500, {})),
    PROFILE_PUT,
    profileStatus,
    selectMoves(POOLING, 'mean'),
    modeSelect,
    'the extra 132a option back',
  )
  await routeGoes(
    () => profileLoaded(() => reply(500, {})),
    PROFILE_PUT,
    profileStatus,
    modeSelect,
    textMoves(CTX, '4096'),
    'the mode select back',
  )
})

// ── 198f: the bar goes quiet, not loud ────────────────────────────────────

test('198f, the Server page: 198h: after the 503, Port typed back. The focus stays on Port, no focusable control is removed, nothing is announced, the status line is empty and Save stays disabled; typing 8082 shows no alert', async () => {
  const { bff, main } = await serverLoaded(() =>
    reply(503, { key: 'client-unavailable', args: [] }),
  )
  const status = serverStatus(main)
  const port = box(main, PORT)
  typeInput(port, PORT_DIRTY)
  port.focus()
  await sent(main, bff, SERVER_PUT)
  await within(main).findByText(UNAVAILABLE)
  expect(document.activeElement, 'focus on Port while it answers').toBe(port)
  const was = focusable()
  expect(was.length, 'the page has focusable controls').toBeGreaterThan(0)
  typeInput(port, PORT_LOADED)
  expectQuiet(main, status, port, was, UNAVAILABLE, 'the alert that went')
  expectNoAlert(status, 'the clean form')
  // Port 8082: the form is dirty again, and the alert is not back (198h).
  typeInput(port, '8082')
  expectDirty(main, status, 'the form at 8082')
  expectNotAnnounced(main, UNAVAILABLE, 'the form at 8082')
  expectNoAlert(status, 'the form at 8082')
})

test('198f, the model profile page: the alert going leaves the focus on the select just used, keeps every focusable control focusable, announces nothing, leaves the status line empty and keeps Save disabled', async () => {
  const { bff, main } = await profileDirty(() => reply(500, {}))
  const status = profileStatus(main)
  const mode = selectOf(main, MODE)
  mode.focus()
  await sent(main, bff, PROFILE_PUT)
  await within(main).findByText(FAILED)
  expect(document.activeElement, 'focus on the mode select').toBe(mode)
  const was = focusable()
  expect(was.length, 'the page has focusable controls').toBeGreaterThan(0)
  pick(main, MODE, AS_LOADED['embeddings'])
  expectQuiet(main, status, mode, was, FAILED, 'the alert that went')
  expectNoAlert(status, 'the clean form')
})

// ── 198g: an answer that lands on a clean form ────────────────────────────

/** The kinds that answer a save: `appear` is what the answer sets while the
 * form is dirty; `stays`, with its `action` button, is what a clean form
 * keeps; null means the kind sets nothing once the form is clean (198g: a
 * clean form never shows a buttonless alert). */
type Answer = {
  what: string
  answer: () => Response
  appear: string
  stays: string | null
  action?: 'sign-in' | 'reload'
}

/** The loop of 198g on one page: three answers, each on a page of its own. */
async function cleanAnswers(
  rows: Answer[],
  open: (answer: () => Response) => Promise<Page>,
  put: string,
  statusOf: (main: HTMLElement) => HTMLElement,
  makeClean: (main: HTMLElement) => void,
) {
  for (const row of rows) {
    cleanup()
    const { bff, main } = await open(row.answer)
    const status = statusOf(main)
    await sent(main, bff, put)
    await within(main).findByText(row.appear)
    expectAlert(status, row.appear, `${row.what} while dirty`)
    makeClean(main)
    expectClean(main, status, `${row.what} clean`)
    if (row.stays === null) {
      expectNotAnnounced(main, row.appear, row.what)
      expectNoAlert(status, `${row.what} on a clean form`)
    } else {
      expectAlert(status, row.stays, `${row.what} on a clean form`)
      expectAction(
        status,
        row.action === 'sign-in' ? SIGN_IN : RELOAD,
        `${row.what} on a clean form`,
      )
    }
  }
}

test('198g, the Server page: an answer that arrives once the form is clean sets what its kind sets and nothing else', async () => {
  await cleanAnswers(
    [
      {
        what: 'a 500 (save-failed)',
        answer: () => reply(500, {}),
        appear: FAILED,
        stays: null,
      },
      {
        what: 'a 503 client-unavailable (api-client-unavailable)',
        answer: () => reply(503, { key: 'client-unavailable', args: [] }),
        appear: UNAVAILABLE,
        stays: null,
      },
      {
        what: 'a 401 (session-ended)',
        answer: () => reply(401, { key: 'session-required', args: [] }),
        appear: ENDED,
        stays: ENDED,
        action: 'sign-in',
      },
    ],
    (answer) => serverDirty(PORT_DIRTY, answer),
    SERVER_PUT,
    serverStatus,
    portBack,
  )
})

test('198g, the model profile page: an answer that arrives once the form is clean sets what its kind sets and nothing else', async () => {
  await cleanAnswers(
    [
      {
        what: 'a 500 (save-failed)',
        answer: () => reply(500, {}),
        appear: FAILED,
        stays: null,
      },
      {
        what: 'a 503 client-unavailable (api-client-unavailable)',
        answer: () => reply(503, { key: 'client-unavailable', args: [] }),
        appear: UNAVAILABLE,
        stays: null,
      },
      {
        what: 'a 409 (api-model-profile-changed-elsewhere)',
        answer: () =>
          reply(409, { key: 'model-profile-changed-elsewhere', args: [] }),
        appear: en('api-model-profile-changed-elsewhere'),
        stays: en('api-model-profile-changed-elsewhere'),
        action: 'reload',
      },
    ],
    profileDirty,
    PROFILE_PUT,
    profileStatus,
    (main) => pick(main, MODE, AS_LOADED['embeddings']),
  )
})

/** Flushes the answer's promise chain, so a "no alert" check runs after the
 * answer was read (the answer of a held PUT arrives outside an act). */
async function settled() {
  await act(async () => {
    for (let i = 0; i < 25; i += 1) await Promise.resolve()
  })
}

/** 198h's last scenario, on one page: the edit undone while the held PUT runs
 * (the bar says save-saving), then the 500 lands on a clean form: no alert,
 * nothing announced, the status line empty (198g, 01's 33b). */
async function answerOnClean(
  open: (gate: Promise<Response>) => Promise<Page>,
  put: string,
  statusOf: (main: HTMLElement) => HTMLElement,
  makeDirty: (main: HTMLElement) => void,
  makeClean: (main: HTMLElement) => void,
) {
  const gate = deferred()
  const { bff, main } = await open(gate.promise)
  const status = statusOf(main)
  const save = saveButton(main)
  makeDirty(main)
  fireEvent.click(save)
  await waitFor(() => expect(bff.to('PUT', put)).toHaveLength(1))
  expect(save.textContent, 'the button says the save is running').toBe(
    en('save-saving'),
  )
  expect(status.textContent, 'the bar says the save is running').toBe(
    en('save-saving'),
  )
  makeClean(main)
  expect(
    save.getAttribute('aria-disabled'),
    'the clean form while the PUT runs: Save disabled',
  ).toBe('true')
  await act(async () => {
    gate.resolve(reply(500, {}))
  })
  await settled()
  expectNotAnnounced(main, FAILED, 'the 500 on a clean form')
  expectClean(main, status, 'the 500 on a clean form')
  expectNoAlert(status, 'the 500 on a clean form')
}

test('198g, the Server page: 198h: a PUT held pending, the edit undone while it runs, then a 500: no alert, the status line empty', async () => {
  await answerOnClean(
    (gate) => serverLoaded(() => gate),
    SERVER_PUT,
    serverStatus,
    (main) => typeInput(box(main, PORT), PORT_DIRTY),
    portBack,
  )
})

test('198g, the model profile page: 198h: a PUT held pending, the edit undone while it runs, then a 500: no alert, the status line empty', async () => {
  await answerOnClean(
    (gate) => profileLoaded(() => gate),
    PROFILE_PUT,
    profileStatus,
    (main) => pick(main, MODE, MODE_DIRTY),
    (main) => pick(main, MODE, AS_LOADED['embeddings']),
  )
})
