// The one rule that switches a model profile's delete off (brief 22, IP5.3 and
// IP5.6): an instance profile that lists the model is its user, and the profile
// is not deletable while one lists it. The Models list's row − and the model
// profile page's Save bar Delete follow this one rule, so a profile the list
// deletes is a profile the page deletes, and neither is ever the stricter of
// the two.
import { t } from './strings.ts'

/** Is the model profile listed by an instance profile (IP5.0's `usedBy`)? A read
 *  that names no `usedBy`, a file of an earlier release, is a profile nobody
 *  lists: the delete acts. */
export function modelProfileInUse(usedBy: string[] | undefined): boolean {
  return (usedBy ?? []).length > 0
}

/** The reason a delete cannot act, the words both pages describe it with
 *  (22 D4.2, 22 L2.B2 b), and null when the delete can. */
export function modelProfileInUseReason(
  usedBy: string[] | undefined,
): string | null {
  return modelProfileInUse(usedBy) ? t('model-profiles-delete-in-use') : null
}
