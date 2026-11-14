// The fe's user-visible text lives in strings/en_US.json, linked by key.
import strings from './strings/en_US.json'

const table: Record<string, string> = strings

/**
 * The text of `key`, its `{N}` placeholders filled by position from `args`.
 * A missing key shows as itself and a placeholder with no value stays, as in
 * pai-core's strings.
 */
export function t(key: string, ...args: string[]): string {
  const text = table[key] ?? key
  return text.replace(/\{(\d+)\}/g, (whole, n: string) => args[Number(n)] ?? whole)
}
