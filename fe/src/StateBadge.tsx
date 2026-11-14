// An instance's state as text plus a shape (fe/DESIGN.md, section 5.2,
// "State badge"): never colour alone.
import { t } from './strings.ts'

/** The label of a state, the manager's name shown as is when the page does not know it. */
export function stateLabel(state: string): string {
  switch (state) {
    case 'active':
      return t('instance-state-active')
    case 'inactive':
      return t('instance-state-inactive')
    case 'activating':
      return t('instance-state-activating')
    case 'deactivating':
      return t('instance-state-deactivating')
    case 'failed':
      return t('instance-state-failed')
    default:
      return t('instance-state-other', state)
  }
}

function shape(state: string): string {
  switch (state) {
    case 'active':
      return 'running'
    case 'activating':
    case 'deactivating':
      return 'changing'
    case 'failed':
      return 'failed'
    default:
      return 'stopped'
  }
}

/** The shape of a state, hidden from assistive technology: the text says it. */
export function StateShape({ state }: { state: string }) {
  return <span className={`shape shape-${shape(state)}`} aria-hidden="true" />
}
