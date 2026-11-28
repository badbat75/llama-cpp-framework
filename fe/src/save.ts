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

/** What the PUT answered, in the order of the brief. `restart` is the instances
 * a restart is needed for (16 17d.4): the ids the reply names, empty when it
 * names none. */
export type SaveAnswer =
  | { kind: 'saved'; revision: string; running: boolean; restart: string[] }
  | { kind: 'signed-out' }
  | { kind: 'revoked' }
  | { kind: 'changed-elsewhere' }
  | {
      kind: 'refused'
      text: string
      field?: string
      /** The 422's key and its args: given by `refusalOf`, so a page can place
       * the mark where the key's args point (brief 21, 21 T.12). */
      key?: string
      args?: string[]
    }
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
    // Brief 22, IP5.0: a model profile save answers the instances whose render
    // lists what it saved, as their ids. The ids are the restart list of
    // 16 17d.4 and say the same `running` says as a boolean.
    if (!isRecord(body) || typeof body.revision !== 'string')
      return failed(t('save-failed'))
    const ids = Array.isArray(body.running)
      ? body.running.filter((id): id is string => typeof id === 'string')
      : null
    const flag = typeof body.running === 'boolean' ? body.running : null
    return ids !== null || flag !== null
      ? {
          kind: 'saved',
          revision: body.revision,
          running: flag ?? (ids !== null && ids.length > 0),
          restart:
            ids ??
            (Array.isArray(body.restart)
              ? body.restart.filter(
                  (id): id is string => typeof id === 'string',
                )
              : []),
        }
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
