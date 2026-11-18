// The page's hash routes (fe/DESIGN.md, section 2.3): the bff serves only /
// and /assets/*, so the fe routes on the fragment.

export type Route =
  | { page: 'overview' }
  | { page: 'sign-in' }
  | { page: 'models' }
  | { page: 'model-profile'; id: string }
  | { page: 'instance'; id: string }
  | { page: 'not-found' }

export function parseRoute(hash: string): Route {
  if (hash === '' || hash === '#' || hash === '#/') return { page: 'overview' }
  if (hash === '#/sign-in') return { page: 'sign-in' }
  if (hash === '#/models') return { page: 'models' }
  const modelProfile = /^#\/models\/([^/]+)$/.exec(hash)
  if (modelProfile) {
    try {
      return { page: 'model-profile', id: decodeURIComponent(modelProfile[1]!) }
    } catch {
      return { page: 'not-found' }
    }
  }
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

/** The link to a model profile's page. */
export function modelProfileHref(id: string): string {
  return `#/models/${encodeURIComponent(id)}`
}

/**
 * True of an id no read can name: the URL parser drops a `.` path segment and
 * resolves `..` against the parent, so `/api/profiles/model/<id>` would reach another
 * route (fe/briefs/02-models.md, 59a). The bff refuses both ids, so "there is
 * no such model profile" is true of every model profile the web can serve.
 */
export function modelProfileIdUnreadable(id: string): boolean {
  return id === '.' || id === '..'
}
