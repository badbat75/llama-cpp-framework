// What a save answered, read the same way by both save forms (the Server
// page's section 3, the model profile page's 145 to 149): the shaped 200, the
// session's answers, the keys the two pages share, the page's own keys, the
// check refusals of a 422, and the generic failure. Each page keeps its own
// table of check keys and its own keys: this file holds the order they are
// read in, and the texts the two pages agree on.
// The text is in en_US.json.
import { apiError, isRecord, sessionRevoked, type Reply } from './api.ts'
import { refusalOf, type RefusalTable } from './refusal.ts'
import { t } from './strings.ts'

/** What the PUT answered, in the order of the brief. */
export type SaveAnswer =
  | { kind: 'saved'; revision: string; running: boolean }
  | { kind: 'signed-out' }
  | { kind: 'revoked' }
  | { kind: 'changed-elsewhere' }
  | { kind: 'refused'; text: string; field?: string }
  | { kind: 'failed'; text: string }

/** What a write that threw reads as: a time-out may have written the file
 * (4.8, 47d), so it is not the generic failure. */
export function sendFailure(e: unknown): string {
  const timedOut = e instanceof DOMException && e.name === 'TimeoutError'
  return timedOut ? t('save-timeout') : t('save-failed')
}

/** Reads `reply`: first the keys the two pages share, then the page's own
 * `own` (its answer of a key, its id filled in), then the 422 table. Anything
 * none of them names is the page's generic failure. */
export function saveAnswer(
  reply: Reply,
  own: (key: string, args: string[]) => SaveAnswer | undefined,
  refusals: RefusalTable,
): SaveAnswer {
  const failed = (text: string): SaveAnswer => ({ kind: 'failed', text })
  if (reply.status === 401) return { kind: 'signed-out' }
  if (sessionRevoked(reply)) return { kind: 'revoked' }
  const body = reply.body
  if (reply.ok) {
    return isRecord(body) &&
      typeof body.revision === 'string' &&
      typeof body.running === 'boolean'
      ? { kind: 'saved', revision: body.revision, running: body.running }
      : failed(t('save-failed'))
  }
  const error = apiError(body)
  if (error === null) return failed(t('save-failed'))
  switch (error.key) {
    case 'client-not-authorized':
      return failed(t('api-client-not-authorized'))
    case 'client-unavailable':
      return failed(t('api-client-unavailable'))
    case 'client-timeout':
      // The service's own time-out may have written the file too (4.8).
      return failed(t('save-timeout'))
  }
  const page = own(error.key, error.args)
  if (page !== undefined) return page
  const refusal = refusalOf(body, refusals)
  if (refusal !== undefined) return { kind: 'refused', ...refusal }
  if (reply.status === 422)
    return { kind: 'refused', text: t('api-other', error.key) }
  return failed(t('save-failed'))
}
