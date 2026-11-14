// The page's hash routes (fe/DESIGN.md, section 2.3): the bff serves only /
// and /assets/*, so the fe routes on the fragment.

export type Route =
  | { page: 'overview' }
  | { page: 'sign-in' }
  | { page: 'instance'; id: string }
  | { page: 'not-found' }

export function parseRoute(hash: string): Route {
  if (hash === '' || hash === '#' || hash === '#/') return { page: 'overview' }
  if (hash === '#/sign-in') return { page: 'sign-in' }
  const instance = /^#\/instances\/([^/]+)$/.exec(hash)
  if (instance) {
    try {
      return { page: 'instance', id: decodeURIComponent(instance[1]!) }
    } catch {
      return { page: 'not-found' }
    }
  }
  return { page: 'not-found' }
}

/** The link to an instance's page. */
export function instanceHref(id: string): string {
  return `#/instances/${encodeURIComponent(id)}`
}
