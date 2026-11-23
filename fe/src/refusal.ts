// The save refusal the two save forms share: a keyed answer of a save marks
// the field the reply names with that key's text, and the bar's alert tells the
// same text (01's 34, 2h's 142 to 144). Each page keeps its own table of check
// keys, with one literal t() call per key: this file holds no text of its own.
import { useCallback, useState } from 'react'
import { apiError, isRecord } from './api.ts'

/** A page's table of check keys, the briefs' tables: `text` is the key's text,
 * undefined for a key the page does not know; `field` is the field a key names
 * when the reply does not, `args` there for the keys whose field is `args[0]`. */
export type RefusalTable = {
  text: (key: string, args: string[]) => string | undefined
  field: (key: string, args: string[]) => string | undefined
}

/** The refusal a body carries: the table's text for its key, and the field to
 * mark, the reply's when it names one, else the table's fallback. `undefined`
 * for a body that is no shaped refusal, or a key the table does not know: the
 * caller falls back to its own generic message. The key and its `args` come
 * along: a page that marks a control the key's position names, as the Tensor
 * placement card does (brief 21, 21 T.12), reads them there. */
export function refusalOf(
  body: unknown,
  table: RefusalTable,
): { text: string; key: string; args: string[]; field?: string } | undefined {
  const error = apiError(body)
  if (error === null) return undefined
  const text = table.text(error.key, error.args)
  if (text === undefined) return undefined
  const named =
    isRecord(body) && typeof body.field === 'string' ? body.field : undefined
  return {
    text,
    key: error.key,
    args: error.args,
    field: named ?? table.field(error.key, error.args),
  }
}

/** The fields a save marked, by the key of the field, and the clearing of one
 * when it is edited: the edit answers the refusal that named that field, the
 * bar's alert stays for the others to read. */
export function useFieldErrors<K extends string>(): {
  errors: Partial<Record<K, string>>
  /** Replaces them all: the answer of a save, or none. */
  set: (errors: Partial<Record<K, string>>) => void
  /** Drops one field's error, leaving the rest. */
  clear: (key: string) => void
} {
  const [errors, setErrors] = useState<Partial<Record<K, string>>>({})
  const set = useCallback(
    (errors: Partial<Record<K, string>>) => setErrors(errors),
    [],
  )
  const clear = useCallback(
    (key: string) =>
      setErrors((e) => {
        if (!(key in e)) return e
        const rest = { ...e }
        delete rest[key as K]
        return rest
      }),
    [],
  )
  return { errors, set, clear }
}
