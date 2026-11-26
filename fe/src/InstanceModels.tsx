// The instance page's Model profiles section (fe/briefs/16-server-profiles-and-instances.md,
// slice 17e, as its "17e check (2026-11-25)" amends it: 17e.4a, 17e.7a, 17e.8a
// and 17e.10a replace 17e.4, 17e.7, 17e.8 and 17e.10): the model profiles one
// instance serves, read once from `GET /api/instances/<id>/profiles/model`,
// with the section's `*` and one row at a time written through the two
// association routes (fe/briefs/api-contract.md). The section is never read on
// the page's poll of the instances list, and is read again after a write's
// failure, so every control shows what the record holds. The page owns the
// sign-in dialog the section's alert opens, as it owns the tags field's. The
// text is in strings/en_US.json.
import { useCallback, useEffect, useRef, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked, type Reply } from './api.ts'
import { modelProfileHref } from './route.ts'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'

/** The section's `*`: `inherit` when the record carries the key, `none` when
 *  it does not. */
type All = 'inherit' | 'none'

/** The word a row write sends (17e.6). */
type Value = 'none' | 'inherit' | 'on' | 'off'

/** One row, of the three fields the section shows. */
type Row = { id: string; enabled: boolean; effective: boolean }

/** Why a call ended badly, and the text that says so. `signed-out` and
 *  `revoked` are a session that ended, so the section offers a Sign in. */
type Failure = {
  ok: false
  kind: 'signed-out' | 'revoked' | 'refused'
  text: string
}

/** A write of the section's `*`. */
type Written = Failure | { ok: true; running: boolean }

/** A write of one row, with the row served as the record left it. */
type WrittenRow = Failure | { ok: true; running: boolean; effective: boolean }

/** A read: the record, or the text of the failure. */
type Read = Failure | { ok: true; all: All; rows: Row[] }

const JSON_HEADERS = { 'Content-Type': 'application/json' }

/** The section's route: `profile` is a row's id, left out for the section's
 *  own `*`. */
const modelsPath = (id: string, profile?: string) =>
  `/api/instances/${encodeURIComponent(id)}/profiles/model` +
  (profile === undefined ? '' : `/${encodeURIComponent(profile)}`)

/** A row of the read: an id that is no non-empty string leaves the row out, a
 *  `enabled` or `effective` that is no boolean reads false (17e.2). */
function parseRow(value: unknown): Row | null {
  if (!isRecord(value) || typeof value.id !== 'string' || value.id === '')
    return null
  return {
    id: value.id,
    enabled: value.enabled === true,
    effective: value.effective === true,
  }
}

/** The read's body: `all` one of the two words, `profiles` an array of rows;
 *  null for any other body, which is a read that failed (17e.2). */
function parseModels(body: unknown): { all: All; rows: Row[] } | null {
  if (
    !isRecord(body) ||
    (body.all !== 'inherit' && body.all !== 'none') ||
    !Array.isArray(body.profiles)
  )
    return null
  return {
    all: body.all,
    rows: body.profiles.map(parseRow).filter((r): r is Row => r !== null),
  }
}

/** The text of a read's failure (17e.2): the keys this route answers, and
 *  `instance-models-failed` for anything else, which says nothing of the body. */
function readText(id: string, key: string): string {
  switch (key) {
    case 'client-no-instance':
      return t('instance-not-found', id)
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    default:
      return t('instance-models-failed')
  }
}

/** The text of a refused write (17e.8, as 17e.8a amends it): the checks the
 *  manager ran, the refusals of the record, and a 422 key the fe has no text
 *  for. `model-profile-bad-value` and `assignment-malformed` are words the
 *  page never sends, so they are the plain failure. A 503 says the same
 *  whatever its body is: a write that outlived the limit is a 503 too, so its
 *  outcome is unknown, and the read that follows shows the record. */
function writeText(
  id: string,
  status: number,
  key: string,
  args: string[],
): string {
  if (status === 503) return t('api-client-unavailable')
  switch (key) {
    case 'model-profile-over-memory':
      return t('api-model-profile-over-memory', ...args)
    case 'model-profile-model-missing':
      return t('api-model-profile-model-missing', ...args)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...args)
    case 'load-on-startup-unlimited':
      return t('api-load-on-startup-unlimited', ...args)
    case 'no-model-profile-named':
      return t('api-no-model-profile-named', ...args)
    case 'client-no-instance':
      return t('instance-not-found', args[0] ?? id)
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'model-profile-bad-value':
    case 'assignment-malformed':
      return t('request-failed')
    default:
      return status === 422 ? t('api-other', key) : t('request-failed')
  }
}

/** An answer that is not a 2xx: the session that ended, or the refusal. */
function refused(id: string, reply: Reply): Failure {
  if (reply.status === 401)
    return { ok: false, kind: 'signed-out', text: t('session-ended-retry') }
  if (sessionRevoked(reply))
    return { ok: false, kind: 'revoked', text: t('session-revoked') }
  const error = apiError(reply.body)
  return {
    ok: false,
    kind: 'refused',
    text: writeText(id, reply.status, error?.key ?? '', error?.args ?? []),
  }
}

/** `GET /api/instances/<id>/profiles/model`. A 401 and a 403 are the session
 *  that ended, which the section hands to the page (17e.2). */
async function readModels(id: string): Promise<Read> {
  try {
    const reply = await call(modelsPath(id))
    if (reply.status === 401)
      return { ok: false, kind: 'signed-out', text: t('session-ended-retry') }
    if (sessionRevoked(reply))
      return { ok: false, kind: 'revoked', text: t('session-revoked') }
    if (!reply.ok)
      return {
        ok: false,
        kind: 'refused',
        text: readText(id, apiError(reply.body)?.key ?? ''),
      }
    const doc = parseModels(reply.body)
    return doc === null
      ? { ok: false, kind: 'refused', text: t('instance-models-failed') }
      : { ok: true, ...doc }
  } catch {
    return { ok: false, kind: 'refused', text: t('instance-models-failed') }
  }
}

/** `PUT /api/instances/<id>/profiles/model`: the section's `*`. */
async function writeAll(id: string, all: All): Promise<Written> {
  try {
    const reply = await call(modelsPath(id), {
      method: 'PUT',
      headers: JSON_HEADERS,
      body: JSON.stringify({ all }),
    })
    return reply.ok &&
      isRecord(reply.body) &&
      typeof reply.body.running === 'boolean'
      ? { ok: true, running: reply.body.running }
      : refused(id, reply)
  } catch {
    return { ok: false, kind: 'refused', text: t('request-failed') }
  }
}

/** `PUT /api/instances/<id>/profiles/model/<profile>`: one row. The reply's
 *  `effective` is the row as the write left it, so the switch takes that and
 *  not the state asked (17e.7). */
async function writeRow(
  id: string,
  profile: string,
  value: Value,
): Promise<WrittenRow> {
  try {
    const reply = await call(modelsPath(id, profile), {
      method: 'PUT',
      headers: JSON_HEADERS,
      body: JSON.stringify({ value }),
    })
    const row = isRecord(reply.body) ? parseRow(reply.body.effective) : null
    return reply.ok &&
      isRecord(reply.body) &&
      typeof reply.body.running === 'boolean' &&
      row !== null
      ? { ok: true, running: reply.body.running, effective: row.effective }
      : refused(id, reply)
  } catch {
    return { ok: false, kind: 'refused', text: t('request-failed') }
  }
}

/** The word that leaves a row served, or not, as asked (17e.6): the least
 *  binding word of the record, so a row that follows the list's own switch
 *  keeps following it. */
function wordFor(row: Row, on: boolean, all: All): Value {
  if (on) return row.enabled ? 'inherit' : 'on'
  return all === 'inherit' && row.enabled ? 'off' : 'none'
}

/** The line a row write leaves (17e.7, as 17e.7a amends it): the page's header
 *  carries the Restart, so a running instance is told to restart and is
 *  offered nothing here. */
function rowText(id: string, effective: boolean, running: boolean): string {
  if (running)
    return effective
      ? t('instance-models-on-restart', id)
      : t('instance-models-off-restart', id)
  return effective ? t('instance-models-on', id) : t('instance-models-off', id)
}

/**
 * The section. `fieldId` names its controls: `-all` the checkbox, `-name-<i>`
 * the span that names row `i`'s switch (17e.4a), `-switch-<i>` that switch,
 * `-sign-in` the Sign in of an ended session. `renewals` is raised by every
 * sign-in the page answers: the section reads again for the new session, and an
 * ended-session alert goes with it. `onSignIn` is given the id of the control
 * the write was made with, so the page puts the focus back on it when its
 * dialog answers (17e.10a); `onSignedOut` ends the app's session when the
 * section's own read says it is gone (17e.2).
 */
export function InstanceModels({
  id,
  fieldId,
  renewals,
  onSignIn,
  onSignedOut,
}: {
  id: string
  fieldId: string
  renewals: number
  onSignIn: (controlId: string) => void
  onSignedOut: (reason: SignedOut) => void
}) {
  const [all, setAll] = useState<All>('inherit')
  // The rows, in the reply's order; null while the read is awaited.
  const [rows, setRows] = useState<Row[] | null>(null)
  // The opening read's failure: the section's plain paragraph, with no control.
  const [failed, setFailed] = useState<string | null>(null)
  // One write at a time (17e.5): the checkbox and every switch are aria-disabled
  // while it runs, and a change sends nothing.
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  // The alert, and the sign-in count it was raised at: a sign-in the page kept
  // raises that count, and the alert goes with it.
  const [alert, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  // The control a write was made with, for the page's dialog.
  const used = useRef('')
  const shown = alert === null || alert.at === renewals ? alert : null

  /** The record, put over the controls. */
  const settle = useCallback((r: Read): void => {
    if (!r.ok) return
    setAll(r.all)
    setRows(r.rows)
  }, [])

  // The one read of the opening (17e.1), and the read of every sign-in the page
  // answers (17e.2): the page's poll of the instances list never touches it, and
  // a 401 or a 403 here ends the session as the page's first read does.
  useEffect(() => {
    let live = true
    readModels(id).then((r) => {
      if (!live) return
      if (r.ok) {
        settle(r)
        setFailed(null)
        return
      }
      if (r.kind === 'signed-out') onSignedOut('ended')
      else if (r.kind === 'revoked') onSignedOut('revoked')
      else setFailed(r.text)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut, renewals, settle])

  /** The record again, after a write (17e.8): every control shows what the file
   *  holds. A read that fails keeps the alert the write raised, the section
   *  being open: its text says what was asked, and a read that says the session
   *  is gone adds the Sign in the write's own answer did not carry. */
  const reread = useCallback(
    async (at: number) => {
      const r = await readModels(id)
      if (r.ok) settle(r)
      else
        setAlert((a) =>
          a === null
            ? { text: r.text, signIn: r.kind !== 'refused', at }
            : { ...a, signIn: a.signIn || r.kind !== 'refused' },
        )
    },
    [id, settle],
  )

  /** One row's switch (17e.6, 17e.7): it moves at once, the write answers the
   *  row as the record left it, and a refusal puts the switch back at what it
   *  showed and reads the record again. The focus stays on the switch. */
  const changeRow = useCallback(
    async (i: number, on: boolean) => {
      const row = rows?.[i]
      if (busy || row === undefined) return
      used.current = `${fieldId}-switch-${i}`
      setRows((rs) =>
        rs === null
          ? rs
          : rs.map((r, n) => (n === i ? { ...r, effective: on } : r)),
      )
      setBusy(true)
      setAlert(null)
      setStatus(t('instance-models-saving'))
      const result = await writeRow(id, row.id, wordFor(row, on, all))
      if (result.ok) {
        setRows((rs) =>
          rs === null
            ? rs
            : rs.map((r, n) =>
                n === i ? { ...r, effective: result.effective } : r,
              ),
        )
        setStatus(rowText(row.id, result.effective, result.running))
        setBusy(false)
        return
      }
      setRows((rs) =>
        rs === null
          ? rs
          : rs.map((r, n) =>
              n === i ? { ...r, effective: row.effective } : r,
            ),
      )
      setAlert({
        text: result.text,
        signIn: result.kind !== 'refused',
        at: renewals,
      })
      setStatus('')
      await reread(renewals)
      setBusy(false)
    },
    [all, busy, fieldId, id, renewals, reread, rows],
  )

  /** The section's `*` (17e.3): the box moves at once, the write answers, and
   *  the record is read again, so the box and the rows show the file. A
   *  refusal puts the box back at what it showed before that read. */
  const changeAll = useCallback(
    async (on: boolean) => {
      if (busy) return
      const prev = all
      const next: All = on ? 'inherit' : 'none'
      used.current = `${fieldId}-all`
      setAll(next)
      setBusy(true)
      setAlert(null)
      setStatus(t('instance-models-saving'))
      const result = await writeAll(id, next)
      if (result.ok) {
        setStatus(
          result.running
            ? t('instance-models-all-restart')
            : on
              ? t('instance-models-all-on')
              : t('instance-models-all-off'),
        )
      } else {
        setAll(prev)
        setAlert({
          text: result.text,
          signIn: result.kind !== 'refused',
          at: renewals,
        })
        setStatus('')
      }
      await reread(renewals)
      setBusy(false)
    },
    [all, busy, fieldId, id, renewals, reread],
  )

  return (
    <section>
      <h2 id={`${fieldId}-title`}>{t('instance-models-title')}</h2>
      <p>{t('instance-models-intro')}</p>
      {failed !== null && <p>{failed}</p>}
      {failed === null && rows === null && (
        <p role="status" className="instance-models-status">
          {t('instance-models-loading')}
        </p>
      )}
      {failed === null && rows !== null && (
        <>
          {/* The section's `*` is a statement, so it is a checkbox with its own
              text in a group named by the label (fe/DESIGN.md "Switch"), and no
              About toggle: the intro above says all there is to say. */}
          <fieldset className="field" aria-labelledby={`${fieldId}-all-label`}>
            <legend className="field-label">
              <span id={`${fieldId}-all-label`}>
                {t('instance-models-all-label')}
              </span>
            </legend>
            <div className="field-control">
              <label className="check">
                <input
                  id={`${fieldId}-all`}
                  type="checkbox"
                  checked={all === 'inherit'}
                  aria-disabled={busy || undefined}
                  onChange={(e) => void changeAll(e.target.checked)}
                />
                {t('instance-models-all')}
              </label>
            </div>
          </fieldset>
          {rows.length === 0 ? (
            <p>{t('instance-models-empty')}</p>
          ) : (
            <div className="table-scroll">
              <table
                className="model-profiles"
                aria-labelledby={`${fieldId}-title`}
              >
                <thead>
                  <tr>
                    <th scope="col">{t('model-profiles-column-name')}</th>
                    <th scope="col">{t('instance-models-column-pool')}</th>
                    <th scope="col">{t('instance-models-column-here')}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row, i) => (
                    <tr key={row.id}>
                      <td>
                        <span id={`${fieldId}-name-${i}`}>
                          <a href={modelProfileHref(row.id)}>{row.id}</a>
                        </span>
                      </td>
                      {/* The state columns sit together at the right (16 17e.13):
                          the cell is as narrow as its content and one line, as
                          the switch's cell is, so the Name column takes the rest. */}
                      <td className="muted status-cell">
                        {row.enabled
                          ? t('model-profiles-status-on')
                          : t('model-profiles-status-off')}
                      </td>
                      {/* The switch and its word are one line, carried by their
                          own span (17e.4a, 22 D4.25): the cell stays a plain
                          table cell, so it centres its content in the row. */}
                      <td className="status-cell">
                        <span>
                          <label className="model-profiles-switch">
                            <input
                              id={`${fieldId}-switch-${i}`}
                              type="checkbox"
                              role="switch"
                              checked={row.effective}
                              aria-labelledby={`${fieldId}-name-${i}`}
                              aria-disabled={busy || undefined}
                              onChange={(e) =>
                                void changeRow(i, e.target.checked)
                              }
                            />
                          </label>
                          <span aria-hidden="true">
                            {row.effective
                              ? t('model-profiles-status-on')
                              : t('model-profiles-status-off')}
                          </span>
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {/* The write's answer lands after the table (16 17e.12): a line above
              it pushed every row down when it filled, under the pointer and the
              focus. */}
          <p role="status" className="instance-models-status">
            {status}
          </p>
          {shown !== null && (
            <p role="alert" className="instance-models-alert">
              {shown.text}
            </p>
          )}
          {shown?.signIn && (
            <button
              id={`${fieldId}-sign-in`}
              type="button"
              className="secondary"
              onClick={() => onSignIn(used.current)}
            >
              {t('session-sign-in')}
            </button>
          )}
        </>
      )}
    </section>
  )
}
