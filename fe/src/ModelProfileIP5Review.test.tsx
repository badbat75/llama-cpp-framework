// Brief 22, slice IP5, review round (fe/briefs/22-model-sources.md, "IP5",
// IP5.3, IP5.6, IP5.7): the two behaviour defects the review of the slice
// found, pinned as behaviour.
//
// D1: the placement keys left the model profile page (IP5.5), so a value of
// one of them that sits in the file is no field of this page. The page's save
// checks its own fields, not the hidden ones: an invalid `ctx-size` or
// `n-gpu-layers` the page never shows must not block the save of a field it
// does show, and the bar must not mark a field that is not on the page.
//
// D2: the delete guard. IP5.3 switches the list's − off when an instance
// profile lists the model, with `model-profiles-delete-in-use`; IP5.6 gives
// the page's Save bar Delete the same rule. The two switch off in exactly the
// same cases and carry exactly the same words, and the page is never the
// stricter of the two: a read that names no usedBy (a file of an earlier
// release) is a profile nobody lists, deletable from the list and from the
// page alike.
//
// A1: IP5.7 says the form never sends a placement key. A placement key that
// sits in the file is no field of this page (IP5.5), so its value, in whatever
// spelling the file holds it, is no edit either: the page loads clean on it,
// and the body of a save of a field the page shows names that field alone.
//
// A2: the two save forms read their answer with the same parser (`save.ts`).
// IP5.0 gives the model profile save a `running` of instance ids; the Server
// page's save keeps answering the boolean it answers today, and the parser
// keeps reading it: the bar says saved, names the running instance and offers
// its Restart.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  INSTANCES,
  MODEL_PROFILE_ID,
  en,
  fakeBff,
  instanceName,
  modelProfileDoc,
  reply,
  startAt,
  type ModelProfile,
} from './test/bff.ts'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (the shapes of ModelProfilesIP5.test.tsx and
// ModelProfilePageIP5.test.tsx, kept local so the pins stand on their own) ──

/** A profile row: `usedBy` are the instance profiles that list it (IP5.0). */
function profile(id: string, usedBy: string[] = []): ModelProfile {
  return {
    id,
    engine: 'llama.cpp',
    engineAvailable: true,
    usedBy,
    model: `/m/${id}.gguf`,
  }
}

/** The Models list, read from the stub. */
async function list(options: Parameters<typeof fakeBff>[0] = {}) {
  startAt('#/profiles/model')
  const bff = fakeBff(options)
  render(<App />)
  await screen.findByRole('heading', { name: en('model-profiles-title') })
  const main = screen.getByRole('main')
  await waitFor(() =>
    expect(main.querySelector('tbody tr'), 'the row').not.toBeNull(),
  )
  return { bff, main }
}

/** The row of the named profile. */
function rowOf(id: string): HTMLElement {
  const main = screen.getByRole('main')
  const row = [...main.querySelectorAll<HTMLElement>('tbody tr')].find((tr) =>
    (tr.textContent ?? '').includes(id),
  )
  expect(row, `the row of ${id}`).not.toBeUndefined()
  return row!
}

/** The row's −, named by the list's own words. */
function minusOf(row: HTMLElement, id: string): HTMLElement {
  return within(row).getByRole('button', {
    name: en('model-profiles-delete-row', id),
  })
}

/** The element an element's `aria-describedby` ids, as its text. */
function describedText(el: HTMLElement): string {
  const ids = el.getAttribute('aria-describedby')
  expect(ids, 'the verb carries its reason as its description').toBeTruthy()
  return (ids ?? '')
    .split(' ')
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ')
}

/** The model profile page, read from the stub. `values` join the document's
 *  and `usedBy` are the instance profiles that list the model (IP5.0). */
async function page(
  values: Record<string, string> = {},
  usedBy: string[] = [],
) {
  startAt(`#/profiles/model/${MODEL_PROFILE_ID}`)
  const bff = fakeBff({
    modelProfile: () =>
      reply(200, {
        ...modelProfileDoc({ pooling: 'mean', ...values }, { usedBy }),
        enabled: undefined,
      }),
  })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: MODEL_PROFILE_ID })
  const main = screen.getByRole('main')
  // The cards landed: Assets first, as IP5.5 keeps it.
  await screen.findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The Save bar's Delete. */
function pageDelete(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
}

/** The bar's alerts, as text. */
function barAlerts(main: HTMLElement): string[] {
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').toBeTruthy()
  return within(bar as HTMLElement)
    .queryAllByRole('alert')
    .map((a) => (a.textContent ?? '').replace(/\s+/g, ' ').trim())
}

// ── D1: the save checks this page's fields, not the hidden placement ones ──

test('IP5.7: the save of a shown field goes through while the file keeps invalid values of placement keys the page no longer shows (IP5 review D1)', async () => {
  // An earlier release wrote these keys; IP5.5 moved them to the instance
  // profile and IP5.7 says the form never sends one. The file keeps them, and
  // the values in it are invalid (ctx-size below 0, n-gpu-layers not a
  // number). The page shows neither field.
  const { bff, main } = await page({ 'ctx-size': '-1', 'n-gpu-layers': 'x' })
  const pooling = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  fireEvent.change(pooling, { target: { value: 'rank' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() =>
    expect(
      bff.calls.filter(
        (c) =>
          c.method === 'PUT' &&
          c.path === `/api/profiles/model/${MODEL_PROFILE_ID}`,
      ).length,
      'IP5.7: the page owns these keys no more, so a value of one that sits in the file is no field of this page and cannot block the save of one it shows',
    ).toBeGreaterThanOrEqual(1),
  )
})

test('IP5.7: a save marks no field the page does not show (IP5 review D1)', async () => {
  // The same file. The save of a shown field is not blocked, so the bar never
  // says "correct the marked fields" pointing at fields this page has none of:
  // no alert shows, and no field of the page carries an error.
  const { bff, main } = await page({ 'ctx-size': '-1', 'n-gpu-layers': 'x' })
  const pooling = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  fireEvent.change(pooling, { target: { value: 'rank' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() =>
    expect(
      bff.calls.filter(
        (c) =>
          c.method === 'PUT' &&
          c.path === `/api/profiles/model/${MODEL_PROFILE_ID}`,
      ).length,
      'the save went through',
    ).toBeGreaterThanOrEqual(1),
  )
  await waitFor(() => expect(barAlerts(main), 'no alert').toEqual([]))
})

// ── D2: one delete rule for the list and the page ────────────────────────

test('IP5.3, IP5.6: the list’s − and the page’s Delete switch off on the same rule and describe themselves with the same words (IP5 review D2)', async () => {
  // The same profile, the same usedBy, read by the list and by the page.
  await list({
    modelProfiles: [profile(MODEL_PROFILE_ID, ['default'])],
  })
  const del = minusOf(rowOf(MODEL_PROFILE_ID), MODEL_PROFILE_ID)
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.3: a listed profile’s − is switched off',
  ).toBe('true')
  const listReason = describedText(del)
  cleanup()
  const { main } = await page({}, ['default'])
  const pageDel = pageDelete(main)
  expect(
    pageDel.getAttribute('aria-disabled'),
    'IP5.6: the page’s Delete of the same profile is switched off too',
  ).toBe('true')
  const pageReason = describedText(pageDel)
  expect(
    pageReason,
    'IP5.6: the page tells the same reason the list tells, in the same words',
  ).toBe(listReason)
  expect(
    listReason,
    'IP5.3: and the words are model-profiles-delete-in-use',
  ).toBe(en('model-profiles-delete-in-use'))
})

test('IP5.3, IP5.6: a profile no instance profile lists answers both its − and the page’s Delete switched on, and each opens its confirm (IP5 review D2)', async () => {
  await list({
    modelProfiles: [profile(MODEL_PROFILE_ID, [])],
  })
  const del = minusOf(rowOf(MODEL_PROFILE_ID), MODEL_PROFILE_ID)
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.3: an unlisted profile’s − acts',
  ).not.toBe('true')
  fireEvent.click(del)
  expect(
    document.querySelector('.confirm-row'),
    'IP5.3: its − opens the confirm row',
  ).not.toBeNull()
  cleanup()
  const { main } = await page({}, [])
  const pageDel = pageDelete(main)
  expect(
    pageDel.getAttribute('aria-disabled'),
    'IP5.6: the page’s Delete of the same profile acts',
  ).not.toBe('true')
  fireEvent.click(pageDel)
  expect(
    main.querySelector('.confirm-group'),
    'IP5.6: the page’s Delete opens its confirm, as the list’s − opens its row',
  ).not.toBeNull()
  expect(
    main.textContent,
    'IP5.6: the confirm asks about this profile',
  ).toContain(en('model-profile-delete-question', MODEL_PROFILE_ID))
})

test('IP5.3, IP5.6: a profile an instance profile lists answers neither verb able: neither opens a confirm (IP5 review D2)', async () => {
  await list({
    modelProfiles: [profile(MODEL_PROFILE_ID, ['default'])],
  })
  fireEvent.click(minusOf(rowOf(MODEL_PROFILE_ID), MODEL_PROFILE_ID))
  expect(
    document.querySelector('.confirm-row'),
    'IP5.3: the − of a listed profile opens no confirm row',
  ).toBeNull()
  cleanup()
  const { main } = await page({}, ['default'])
  fireEvent.click(pageDelete(main))
  expect(
    main.querySelector('.confirm-group'),
    'IP5.6: the page’s Delete of the same profile opens no confirm group',
  ).toBeNull()
})

test('IP5.3, IP5.6: a read that names no usedBy answers the list and the page with the same deletability (IP5 review D2)', async () => {
  // A file of an earlier release carries no usedBy anywhere. The list reads
  // such a row as a profile nobody lists, its − acting (IP5.3's rule). The
  // page applies the same rule to the same read: it reads the document, and
  // its Delete of the same profile is switched on, and opens its confirm. The
  // page is never the stricter of the two: a profile the list deletes is a
  // profile the page deletes.
  await list({
    modelProfiles: [
      {
        id: MODEL_PROFILE_ID,
        engine: 'llama.cpp',
        engineAvailable: true,
        model: `/m/${MODEL_PROFILE_ID}.gguf`,
      },
    ],
  })
  const del = minusOf(rowOf(MODEL_PROFILE_ID), MODEL_PROFILE_ID)
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.3: a row whose read names no usedBy is a row nobody lists, so its − acts',
  ).not.toBe('true')
  fireEvent.click(del)
  expect(
    document.querySelector('.confirm-row'),
    'IP5.3: and its − opens the confirm row',
  ).not.toBeNull()
  cleanup()
  startAt(`#/profiles/model/${MODEL_PROFILE_ID}`)
  fakeBff({
    modelProfile: () =>
      reply(200, {
        ...modelProfileDoc({}, { usedBy: undefined }),
        enabled: undefined,
      }),
  })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: MODEL_PROFILE_ID })
  const main = screen.getByRole('main')
  const pageDel = await screen
    .findByRole('button', { name: en('model-profile-delete') })
    .catch(() => null)
  expect(
    pageDel,
    'IP5.6: the page reads the same document the list reads (a missing usedBy is a profile nobody lists), so its page shows the Delete the list shows, not a read failure',
  ).not.toBeNull()
  expect(
    pageDel!.getAttribute('aria-disabled'),
    'IP5.6: the page’s Delete of the same profile is switched on, as the list’s − is',
  ).not.toBe('true')
  fireEvent.click(pageDel!)
  expect(
    main.querySelector('.confirm-group'),
    'IP5.6: and it opens the page’s confirm, as the list’s − opens its row',
  ).not.toBeNull()
})

test('IP5.3, IP5.6: the switched-off − and the switched-off Delete both float the same words on focus (IP5 review D2)', async () => {
  await list({
    modelProfiles: [profile(MODEL_PROFILE_ID, ['default'])],
  })
  const del = minusOf(rowOf(MODEL_PROFILE_ID), MODEL_PROFILE_ID)
  fireEvent.focus(del)
  const listTip = document.querySelector('.hint-tip')
  expect(
    listTip,
    'IP5.3: the − floats its reason on focus (22 D4.13)',
  ).not.toBeNull()
  cleanup()
  const { main } = await page({}, ['default'])
  const pageDel = pageDelete(main)
  fireEvent.focus(pageDel)
  const pageTip = main.querySelector('.hint-tip')
  expect(
    pageTip,
    'IP5.6: the page’s Delete floats its reason on focus, as the list’s − does (22 L2.B2 c)',
  ).not.toBeNull()
  expect(
    (pageTip!.textContent ?? '').replace(/\s+/g, ' ').trim(),
    'IP5.6: the tip says exactly what the list’s tip says',
  ).toBe((listTip!.textContent ?? '').replace(/\s+/g, ' ').trim())
})

// ── A1: a placement key that sits in the file is no field of this page ──

/** The Save bar's Save changes. */
function saveChanges(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** The PUTs of this model profile, in order. */
function putsOf(bff: ReturnType<typeof fakeBff>) {
  return bff.calls.filter(
    (c) =>
      c.method === 'PUT' &&
      c.path === `/api/profiles/model/${MODEL_PROFILE_ID}`,
  )
}

/** The `values` of the n-th PUT of this model profile. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const puts = putsOf(bff)
  expect(
    puts.length,
    `the save went through ${n} time(s)`,
  ).toBeGreaterThanOrEqual(n)
  return (puts[n - 1]!.body as Record<string, unknown>).values as Record<
    string,
    unknown
  >
}

/** Ticks a number field's default box: the field goes to its default, which
 *  for a key the file holds is the key's removal (156). */
function tickDefault(key: string) {
  const box = document.getElementById(`model-profile-${key}-box`)
  expect(box, `the default box of ${key}`).not.toBeNull()
  fireEvent.click(box!)
}

test('IP5.7: a placement key the file leaves empty loads clean, and a save of a field the page shows sends that field alone (IP5 review A1)', async () => {
  // An earlier release wrote `fit-target` and left it empty. IP5.5 moved the
  // key to the instance profile, so this page has no field for it, and IP5.7
  // says the form never sends one. The value is not an edit, and the body of
  // the save of `cache-reuse`, a field the page does show, names it alone.
  const { bff, main } = await page({ 'fit-target': '', 'cache-reuse': '512' })
  expect(
    saveChanges(main).getAttribute('aria-disabled'),
    'IP5.7: a key the page shows no field for is no edit: the page loads clean, its Save switched off',
  ).toBe('true')
  tickDefault('cache-reuse')
  fireEvent.click(saveChanges(main))
  await waitFor(() =>
    expect(
      putsOf(bff),
      'the save of the field the page shows goes through',
    ).toHaveLength(1),
  )
  expect(
    sentValues(bff),
    'IP5.7: the form never sends a placement key: the body holds the one field it shows',
  ).toEqual({ 'cache-reuse': null })
})

test('IP5.7: a placement key the file pads with spaces loads clean, and its spelling is never sent (IP5 review A1)', async () => {
  // The same key, held as a text padded with spaces. The page trims the text
  // of a field it shows (166); the key has no field here, so the file's
  // spelling is nothing the form composes: the page loads clean and the save
  // of another field sends that field alone, with no trimmed copy of the key.
  const { bff, main } = await page({
    'fit-target': '  1024,2048  ',
    'cache-reuse': '512',
  })
  expect(
    saveChanges(main).getAttribute('aria-disabled'),
    'IP5.7: the file’s spaces in a key with no field on this page are no edit: the page loads clean',
  ).toBe('true')
  tickDefault('cache-reuse')
  fireEvent.click(saveChanges(main))
  await waitFor(() =>
    expect(
      putsOf(bff),
      'the save of the field the page shows goes through',
    ).toHaveLength(1),
  )
  expect(
    sentValues(bff),
    'IP5.7: the save sends no placement key, not even the key’s text trimmed',
  ).toEqual({ 'cache-reuse': null })
})

// ── A2: the shared parser keeps reading the Server page's boolean running ─

test('IP5.0: the Server page’s save answers a boolean running, and the shared parser reads it as saved with that instance named (IP5 review A2)', async () => {
  // IP5.0 gives the model profile save a `running` of instance ids. The Server
  // page's save keeps the boolean it answers today, and `save.ts` is read by
  // both pages: the boolean must keep landing as saved, the bar naming the
  // running instance and offering its Restart (16 17d.3a), the form clean.
  const name = instanceName(INSTANCES[0]!)
  startAt('#/profiles/server/default')
  const bff = fakeBff({
    serverProfileSave: () => reply(200, { revision: 'r-2', running: true }),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  fireEvent.change(
    within(main).getByRole('textbox', { name: en('server-port') }),
    { target: { value: '8581' } },
  )
  fireEvent.click(saveChanges(main))
  const bar = main.querySelector('.save-bar')
  expect(bar, 'the Save bar').not.toBeNull()
  await waitFor(() =>
    expect(
      (bar as HTMLElement).textContent,
      `IP5.0: a boolean running is read as saved, the bar naming the running ${name}`,
    ).toContain(en('save-saved-restart-ids', name)),
  )
  expect(
    saveChanges(main).getAttribute('aria-disabled'),
    'IP5.0: the answer is read as saved: the form is clean',
  ).toBe('true')
  expect(
    within(main).getByRole('button', {
      name: en('restart-offer-button', name),
    }),
    'IP5.0: and its Restart offer stands in the bar',
  ).toBeTruthy()
  expect(
    bff.calls.filter(
      (c) => c.method === 'PUT' && c.path === '/api/profiles/server/default',
    ),
    'IP5.0: the save went to the Server profile route, the page the boolean belongs to',
  ).toHaveLength(1)
})
