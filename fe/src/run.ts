// The run verbs of an instance, shared by the instance page and the Overview's
// tile (fe/briefs/24-ux-audit.md, 23 A1.1): the same three actions, the same
// words, icons and state rules, and the same text for a failed one. The page
// owns its polls; the tile settles on one fresh read of the shared store
// (23 A1.2, and 1c.41 leaves the Overview unpolluted).
import { apiError, call, sessionRevoked } from './api.ts'
import { type IconName } from './Icon.tsx'
import { t } from './strings.ts'

/** One of the three verbs, in the order every page shows them. */
export type Action = 'start' | 'stop' | 'restart'

export const ACTIONS: Action[] = ['start', 'stop', 'restart']

/** The verb's word. */
export function runLabel(action: Action): string {
  switch (action) {
    case 'start':
      return t('run-start')
    case 'stop':
      return t('run-stop')
    case 'restart':
      return t('run-restart')
  }
}

/** The verb's icon (08-hearth-h8-icons.md H8b.1), beside the word it says. */
export function runIcon(action: Action): IconName {
  switch (action) {
    case 'start':
      return 'play'
    case 'stop':
      return 'square'
    case 'restart':
      return 'rotate-cw'
  }
}

/** The line the page shows while `action` runs. */
export function runningText(action: Action): string {
  switch (action) {
    case 'start':
      return t('run-starting')
    case 'stop':
      return t('run-stopping')
    case 'restart':
      return t('run-restarting')
  }
}

/** The outcome of `action` once a read gives `state`, null while it runs (42a). */
export function settledText(action: Action, state: string): string | null {
  if (state === 'failed') return t('run-ended-failed')
  if (action === 'start' && state === 'active') return t('run-started')
  if (action === 'restart' && state === 'active') return t('run-restarted')
  if (action === 'stop' && state === 'inactive') return t('run-stopped')
  return null
}

/** The buttons a state leaves inactive (40a); an unknown state leaves none. */
export function inactiveIn(state: string, action: Action): boolean {
  switch (state) {
    case 'active':
      return action === 'start'
    case 'inactive':
    case 'failed':
      return action !== 'start'
    case 'activating':
    case 'deactivating':
      return true
    default:
      return false
  }
}

/** Sends the action: 'ok' on any 2xx, else 'signed-out' or the error key. */
export async function sendAction(id: string, action: Action): Promise<string> {
  try {
    const reply = await call(
      `/api/instances/${encodeURIComponent(id)}/${action}`,
      { method: 'POST' },
    )
    if (reply.ok) return 'ok'
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    return apiError(reply.body)?.key ?? 'failed'
  } catch {
    return 'failed'
  }
}

/** The run alert of a failed action, by its key (43). `name` is the name the
 * page shows for the instance, so a refusal that names it says what the eye
 * reads (24 A1.4 D1). */
export function failureText(name: string, key: string): string {
  switch (key) {
    case 'signed-out':
      return t('session-ended-retry')
    case 'revoked':
      return t('session-revoked')
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-no-instance':
      return t('instance-not-found', name)
    // The 17c refusal: trying again cannot help, the instance needs a model
    // profile first.
    case 'instance-no-model-profiles':
      return t('api-instance-no-model-profiles', name)
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'client-service-failed':
      return t('api-client-service-failed')
    // A 504 reached the manager: the write may have happened, and the page
    // reads again to show what the manager holds (16 17f.10).
    case 'client-timeout':
      return t('write-timeout')
    default:
      return t('request-failed')
  }
}
