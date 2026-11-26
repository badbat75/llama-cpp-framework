// The model profile's on/off switch, shared by the Models list's rows (02
// slice 2c) and the model profile page's state line (brief 24, 23 A2): the
// write of `enabled`, the word of a switch that held, and the word of a
// refusal. Each page owns what it does with the answer (the list keeps its
// rows in place, the page reads the document again): this file holds the route
// and the texts the two agree on, so a switch answers the same way on both.
// The text is in en_US.json.
import {
  apiError,
  call,
  isRecord,
  sessionRevoked,
  type ApiError,
} from './api.ts'
import { t } from './strings.ts'

/** What a switch write gave: the reply, 'signed-out', the error, or 'failed'.
 * `restart` is the instances a restart is needed for (16 17d.5): the ids the
 * reply names, empty when it names none. */
export type Written =
  | { enabled: boolean; running: boolean; restart: string[] }
  | 'signed-out'
  | 'revoked'
  | (ApiError & { status: number })
  | 'failed'

export async function writeEnabled(
  id: string,
  enabled: boolean,
): Promise<Written> {
  try {
    const reply = await call(
      `/api/profiles/model/${encodeURIComponent(id)}/enabled`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      },
    )
    if (reply.ok) {
      const body = reply.body
      return isRecord(body) &&
        typeof body.enabled === 'boolean' &&
        typeof body.running === 'boolean'
        ? {
            enabled: body.enabled,
            running: body.running,
            restart: Array.isArray(body.restart)
              ? body.restart.filter(
                  (id): id is string => typeof id === 'string',
                )
              : [],
          }
        : 'failed'
    }
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    const error = apiError(reply.body)
    return error === null ? 'failed' : { ...error, status: reply.status }
  } catch {
    return 'failed'
  }
}

/** The alert of a failed switch write (2c.48). */
export function failureText(
  id: string,
  result: Exclude<Written, { running: boolean }>,
): string {
  if (result === 'signed-out') return t('session-ended-retry')
  if (result === 'revoked') return t('session-revoked')
  if (result === 'failed') return t('request-failed')
  switch (result.key) {
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    // A 504 reached the manager: the write may have happened, and the rows are
    // read again to follow it (16 17d.2a).
    case 'client-timeout':
      return t('write-timeout')
    case 'no-model-profile-named':
      return t('api-no-model-profile-named', id)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...result.args)
    case 'load-on-startup-unlimited':
      return t('api-load-on-startup-unlimited', ...result.args)
    case 'model-profile-over-memory':
      return t('api-model-profile-over-memory', ...result.args)
    case 'model-profile-model-missing':
      return t('api-model-profile-model-missing', ...result.args)
    default:
      return result.status === 422
        ? t('api-other', result.key)
        : t('request-failed')
  }
}

/** The status of a switch that held (2c.46): the words of the profile it
 * switched, with the restart note when its engine runs. */
export function switchedText(
  id: string,
  engine: string,
  enabled: boolean,
  running: boolean,
): string {
  if (enabled) {
    return running
      ? t('model-profiles-switched-on-restart', id, engine)
      : t('model-profiles-switched-on', id, engine)
  }
  return running
    ? t('model-profiles-switched-off-restart', id, engine)
    : t('model-profiles-switched-off', id, engine)
}

/** The status of a switch that held, naming the instances to restart (16
 * 17d.5): `names` is the reply's ids as the app shows them, joined for the
 * text. The engine is left out: the instances are the news. */
export function switchedTextIds(
  id: string,
  enabled: boolean,
  names: string,
): string {
  return enabled
    ? t('model-profiles-switched-on-restart-ids', id, names)
    : t('model-profiles-switched-off-restart-ids', id, names)
}
