// Test helpers for the web UI: a fake bff behind a mocked fetch (no network),
// the English texts of the design brief, and the hash route of the page.
// The shapes are those of fe/briefs/api-contract.md.
import { vi } from 'vitest'

/**
 * The English of the strings keys the tests read, as the design brief gives
 * them (fe/briefs/01-server.md, section 5.5, and the keys that stay;
 * fe/briefs/02-models.md, section 2.7). The tests assert these texts, so a
 * test does not depend on en_US.json holding the key yet.
 */
export const EN: Record<string, string> = {
  'app-name': 'Private AI Framework',
  'page-title': '{0} | Private AI Framework',
  'skip-to-content': 'Skip to content',
  'nav-label': 'Main',
  'nav-menu': 'Menu',
  'nav-overview': 'Overview',
  'nav-instances': 'Instances',
  loading: 'Loading…',
  'not-found': 'There is no page at this address.',
  'not-found-home': 'Go to the overview',
  'session-sign-in': 'Sign in',
  'session-sign-out': 'Sign out',
  'session-signed-in-as': 'Signed in as {0}',
  'session-role-admin': 'Administrator',
  'session-role-user': 'User',
  'signin-title': 'Sign in',
  'signin-intro':
    'Use the user name and password of your account on the computer that runs Private AI Framework. Only members of its pai group can sign in.',
  'signin-required': 'Sign in to see this page.',
  'signin-username': 'User name',
  'signin-password': 'Password',
  'signin-submit': 'Sign in',
  'signin-busy': 'Signing in…',
  'signin-cancel': 'Cancel',
  'signin-empty-username': 'Enter your user name.',
  'signin-empty-password': 'Enter your password.',
  'signin-failed':
    'Signing in failed. Try again; if it keeps failing, the web service may not be running.',
  'api-session-bad-credentials': 'The user name or password is not correct.',
  'api-session-too-many-attempts':
    'Too many attempts. Try again in {0} seconds.',
  'api-session-unavailable':
    'Signing in is not available on this server right now. You can still see the overview without signing in.',
  'api-session-not-administrator':
    'Only members of the pai group can sign in, and this account is not one. Ask whoever administers this computer to add it; you can still see the overview without signing in.',
  'overview-title': 'Overview',
  'instances-column-engine': 'Engine',
  'instance-engine': '{0} instance',
  'instance-state': 'State: {0}',
  'instance-not-found': 'There is no instance named {0}.',
  'server-title': 'Server',
  'server-intro':
    'llama-server settings of this instance, shared by every model it serves ({0}). Per-model settings belong to the presets.',
  'server-loading': 'Loading the server settings…',
  'server-load-failed': 'The server settings could not be read.',
  'server-read-only':
    'You can view these settings. Saving them needs an administrator account.',
  'api-client-not-authorized':
    'Not authorized: the system refused this request.',
  'api-client-unavailable':
    'The Private AI manager service is not running or not reachable.',
  'field-default': 'default',
  'field-auto': 'auto',
  'field-no-key': 'No key',
  'field-about': 'About',
  'field-show': 'Show',
  'option-default': 'default',
  'option-on': 'on',
  'option-off': 'off',
  'option-internal': 'internal',
  'option-none': 'none',
  'server-network': 'Network',
  'server-resources': 'Resources',
  'server-advanced': 'Advanced',
  'server-snapshots': 'Conversation snapshots',
  'server-sdk': 'SDK tuning',
  'server-sdk-intro':
    "Settings of the GPU vendor's own libraries, passed to llama-server as environment variables at launch. They are not llama.cpp flags: nothing checks their values, and each one only affects its own backend.",
  'server-cuda-and-rocm': 'CUDA and ROCm',
  'server-port': 'Port',
  'server-bind-to': 'Bind to',
  'server-bind-localhost': 'localhost (only the computer running the server)',
  'server-bind-all': '0.0.0.0 (all interfaces, LAN-reachable)',
  'server-bind-gone': '{0} (no longer present)',
  'server-base-url': 'Base URL',
  'server-base-url-hint':
    'The address integrations use (opencode.json and Claude Code).\nDefault: built from host and port, with /v1 added for opencode.json; the default shown is that of the saved settings. Untick default to enter your own, for a proxy or gateway.',
  'server-api-key': 'API key',
  'server-api-key-withheld': 'A key is set. Only an administrator can see it.',
  'server-cpu-threads': 'CPU threads',
  'server-cpu-threads-hint':
    "How many CPU threads generate tokens. More won't help once the GPU is the limit.\nDefault: llama.cpp uses the physical core count. Maximum = the logical processors of the computer running the server.\nFlag: --threads",
  'server-batch-threads': 'Batch threads',
  'server-batch-threads-hint':
    'How many CPU threads read the prompt (prefill).\nDefault: the same as CPU threads. Maximum = the logical processors of the computer running the server.\nFlag: --threads-batch',
  'server-max-models': 'Max models',
  'server-memory-margin': 'Memory margin',
  'server-models-folder': 'Models folder',
  'server-models-folder-hint':
    'The folder holding your models, on the computer running the server. The pickers read its subfolders: models, mmprojs, mtps and dflashs.\nLeft empty, the instance uses its own folder, shown in the field.',
  'server-folder-instance-default': 'unset: the instance uses {0}',
  'server-web-ui': 'Web UI',
  'server-web-ui-hint':
    "Serves the built-in chat page at /. With it off, the API keeps working.\nDefault: on (llama.cpp's own).\nFlag: --no-ui",
  'server-web-ui-toggle': 'serve the built-in chat UI',
  'server-mcp-proxy': 'Web UI MCP proxy',
  'server-mcp-proxy-toggle': "serve the web UI's MCP proxy endpoint",
  'server-log-level': 'Log level',
  'server-log-level-hint':
    "How much llama-server writes to its log. Each level includes the ones below it: OUTPUT:0 writes almost nothing, DEBUG:5 everything.\nDefault: TRACE:4, which records each request in the instance's log. DEBUG:5 also logs the per-tensor lines tools\\report-memory.ps1 reads.\nFlag: --log-verbosity",
  'server-save-on-shutdown': 'Save on shutdown',
  'server-save-on-shutdown-toggle':
    'snapshot the KV cache on stop and restore it on start',
  'server-snapshot-folder': 'Snapshot folder',
  'server-profiler-attach': 'Profiler attach',
  'server-profiler-attach-toggle': 'allow rocprofv3 --attach',
  'server-allreduce': 'Tensor-split AllReduce',
  'server-gpu-graphs': 'Disable GPU graphs',
  'server-gpu-graphs-toggle': 'launch kernels one by one',
  'server-kv-rotation': 'KV rotation',
  'server-kv-rotation-toggle': 'disable the KV-cache rotation',
  // Keys that stay as they are.
  'version-label': 'Version {0}',
  'version-unavailable': 'The version could not be read from the service.',
  'instances-title': 'Instances',
  'instances-column-id': 'Instance',
  'instances-column-state': 'State',
  'instances-empty': 'No instances are configured.',
  'instances-failed': 'The instances could not be read.',
  'instances-manager-unavailable':
    'The manager is not running or does not answer.',
  'instances-not-authorized': "The manager refused the web service's request.",
  'instance-state-active': 'Running',
  'instance-state-inactive': 'Stopped',
  'instance-state-activating': 'Starting',
  'instance-state-deactivating': 'Stopping',
  'instance-state-failed': 'Failed',
  'instance-state-other': 'Unknown ({0})',
  // Slice 1b (section 5.6): editing and saving.
  'save-button': 'Save changes',
  'save-discard': 'Discard',
  'save-saving': 'Saving…',
  'save-unsaved': 'You have unsaved changes.',
  'save-saved': 'Saved.',
  'save-saved-restart': 'Saved: restart llama-server to apply.',
  'save-invalid': 'Not saved: correct the marked fields.',
  'save-refused': 'Not saved: {0}',
  'save-failed':
    'Not saved: the service did not answer as expected. Your changes are kept; try again.',
  'save-reload': 'Reload',
  'session-ended':
    'Your session has ended. Sign in again to save; your changes are kept.',
  'discard-title': 'Discard unsaved changes?',
  'discard-description':
    'You have unsaved edits that will be lost if you continue.',
  'discard-keep': 'Keep editing',
  'discard-confirm': 'Discard changes',
  'field-error-number': 'Enter a whole number from {0} to {1}.',
  'field-error-number-min': 'Enter a whole number of at least {0}.',
  'field-error-comment-markers':
    'Remove the ; and # characters: the INI file would cut the value there.',
  'api-not-whole-number-in-range':
    '{0} = `{1}` is not a whole number from {2} to {3}',
  'api-comment-markers':
    "{0} contains ';' or '#', which the INI comment rule would truncate on reload; use a path without those characters",
  'api-load-on-startup-over-max':
    '{0} enabled presets would load on startup, but the server loads at most {1} models at once (ModelsMax) and would not start. Turn Load on startup off on another preset, or raise ModelsMax',
  'api-load-on-startup-unlimited':
    '{0} enabled preset(s) would load on startup with ModelsMax = 0, which llama-server rejects at startup (it reads 0 as a limit there, not as unlimited). Set ModelsMax to at least {0}, or turn Load on startup off',
  'api-client-address-in-use':
    'Another instance, {0}, already uses that address and port.',
  'api-client-unresolvable-host':
    'The host name {0} does not resolve to an address.',
  'api-server-changed-elsewhere':
    'These settings were changed elsewhere after you opened them. Reload to see them; your edits here will be lost.',
  'api-other': 'The request was refused ({0}).',
  'server-edits-dropped':
    'Your unsaved changes were dropped: you signed in with a different account, or with one that cannot save these settings. The form shows the saved settings.',
  // Slice 1c (section 5.7): run controls and live state.
  'run-start': 'Start',
  'run-stop': 'Stop',
  'run-restart': 'Restart',
  'run-starting': 'Starting llama-server…',
  'run-stopping': 'Stopping llama-server…',
  'run-restarting': 'Restarting llama-server…',
  'run-started': 'llama-server started.',
  'run-stopped': 'llama-server stopped.',
  'run-restarted': 'llama-server restarted.',
  'run-ended-failed':
    'llama-server is in the Failed state. Its log on the server says why.',
  'run-timeout':
    'llama-server did not reach the expected state within a minute. The state above keeps updating.',
  'request-failed': 'The request did not go through. Try again.',
  'session-ended-retry': 'Your session has ended. Sign in again, then retry.',
  'api-client-service-failed':
    "The service manager reported an error. The instance's log on the server says why.",
  // Slice 1c (section 5.7): start at boot.
  'boot-label': 'Start at boot',
  'boot-toggle': 'start llama-server when the system starts',
  'boot-hint':
    "Starts llama-server when the system starts, before anyone logs in: the instance's service is enabled (Linux: systemctl enable llama.cpp@default; Windows: its start type is Automatic).",
  'boot-on': 'llama-server will start when the system starts.',
  'boot-off': 'llama-server will no longer start when the system starts.',
  'boot-load-failed': 'Whether llama-server starts at boot could not be read.',
  // Slice 1c (section 5.7, 4.8): a save that timed out.
  'save-timeout':
    'Not confirmed: the service did not answer in time, so the settings may or may not have been saved. Your changes are kept; save again to be sure.',
  // Slice 2a (fe/briefs/02-models.md, section 2.7): the preset list.
  'nav-models': 'Models',
  'models-title': 'Models',
  'presets-title': 'Presets',
  'presets-intro':
    "Each preset is one model with its settings. Every instance of the preset's engine can serve it.",
  'presets-column-name': 'Preset',
  'presets-column-engine': 'Engine',
  'presets-column-status': 'Status',
  'presets-status-on': 'On',
  'presets-status-off': 'Off',
  'presets-engine-not-available': '{0} (not available)',
  'presets-loading': 'Loading the presets…',
  'presets-empty': 'No presets yet.',
  'presets-load-failed': 'The presets could not be read.',
  'presets-off-note':
    'A preset switched off is kept, but its engine does not serve it and OpenCode does not offer it.',
  // Slice 2b (fe/briefs/02-models.md, section 2b.9): the list's filter.
  'presets-filter-label': 'Filter presets',
  'presets-filter-hint':
    'Matches part of the preset name or of its model file path. Case does not matter.',
  'presets-filter-clear': 'Clear filter',
  'presets-filter-count': 'Presets shown: {0} of {1}.',
  'presets-no-match': 'No presets match the filter.',
  // Slice 2c (fe/briefs/02-models.md, section 2c.8): the on/off switch.
  'presets-switched-on': 'Switched {0} on.',
  'presets-switched-off': 'Switched {0} off.',
  'presets-switched-on-restart':
    'Switched {0} on: restart the running instances of {1} to apply.',
  'presets-switched-off-restart':
    'Switched {0} off: restart the running instances of {1} to apply.',
  'api-no-preset-named':
    'There is no preset named {0} any more: it was renamed or deleted elsewhere. Open the Models page again to see the presets as they are.',
}

/** The brief's English of `key`, `{N}` filled by position; a typo throws. */
export function en(key: string, ...args: string[]): string {
  const text = EN[key]
  if (text === undefined) throw new Error(`no English for ${key} in the test`)
  return text.replace(
    /\{(\d+)\}/g,
    (whole, n: string) => args[Number(n)] ?? whole,
  )
}

export function reply(status: number, body?: unknown): Response {
  if (status === 204) return new Response(null, { status })
  const text = typeof body === 'string' ? body : JSON.stringify(body)
  return new Response(text, {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

/** A reply the test settles itself: `promise` waits for `resolve`. */
export function deferred(): {
  promise: Promise<Response>
  resolve: (r: Response) => void
} {
  let resolve: (r: Response) => void = () => {}
  const promise = new Promise<Response>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

/**
 * A reply held like `deferred()`, that also behaves as fetch does when its
 * request is aborted: it rejects with the signal's `reason` (1c.47e).
 * `answer` is passed to the fake bff (it receives the fetch `init`);
 * `signals` holds the signal of each request it answered, in order.
 */
export function held(): {
  answer: (init?: RequestInit) => Promise<Response>
  resolve: (r: Response) => void
  signals: (AbortSignal | undefined)[]
} {
  const signals: (AbortSignal | undefined)[] = []
  let settle: (r: Response) => void = () => {}
  const replied = new Promise<Response>((r) => {
    settle = r
  })
  return {
    signals,
    answer(init?: RequestInit) {
      const signal = init?.signal ?? undefined
      signals.push(signal)
      return new Promise<Response>((resolve, reject) => {
        if (signal?.aborted) {
          reject(signal.reason)
          return
        }
        signal?.addEventListener('abort', () => reject(signal.reason), {
          once: true,
        })
        void replied.then(resolve)
      })
    },
    resolve: (r) => settle(r),
  }
}

/** One instance as /api/instances lists it. */
export type Instance = { id: string; engine: string; state: string }

export const INSTANCES: Instance[] = [
  { id: 'default', engine: 'llama.cpp', state: 'active' },
  { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
]

/** One preset as GET /api/presets lists it (fe/briefs/api-contract.md). */
export type Preset = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  /** The section's `model` value as the file holds it; "" when unset (2b). */
  model: string
}

/** The default preset list: one switched on, one switched off, one engine. */
export const PRESETS: Preset[] = [
  {
    id: 'Qwen3.6-35B-A3B-UD-Q4_K_XL',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    model:
      '/var/lib/private-ai/.llama.cpp/models/Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf',
  },
  {
    id: 'gemma-3-12b-it',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: false,
    model: '',
  },
]

/**
 * A server document (GET /api/instances/{id}/server) with every kind of value.
 * `extra` adds or replaces top-level members, such as `withheld`.
 */
export function serverDoc(
  values: Record<string, unknown> = {},
  extra: Record<string, unknown> = {},
) {
  return {
    revision: 'r-1731628800-42',
    values: {
      Port: 8580,
      Hostname: 'localhost,10.0.0.9,192.168.1.20',
      OpencodeBaseUrl: null,
      OpencodeApiKey: 'sk-local-123',
      Threads: null,
      ThreadsBatch: 16,
      ModelsMax: null,
      ModelsMemoryMargin: 1024,
      ModelsDir: null,
      Webui: true,
      WebuiMcpProxy: false,
      LogVerbosity: 3,
      SaveStateOnShutdown: true,
      StateDir: '/data/llama-state',
      RocblasUseHipblaslt: 'off',
      RocprofAttach: null,
      CudaAllreduce: 'nccl',
      CudaDisableGraphs: false,
      AttnRotDisable: null,
      ...values,
    },
    defaults: {
      Port: 9931,
      ModelsMax: 4,
      ModelsMemoryMargin: 0,
      OpencodeBaseUrl: 'http://127.0.0.1:8580/v1',
      ModelsDir: '/var/lib/llama.cpp/models',
      StateDir: '/var/lib/llama.cpp/state',
    },
    limits: {
      Port: [1, 65535],
      ModelsMax: [0, 2147483647],
      ModelsMemoryMargin: [0, 2147483647],
      Threads: [1, 32],
      ThreadsBatch: [1, 32],
    },
    interfaces: [{ value: '192.168.1.20', label: 'enp5s0 (192.168.1.20)' }],
    ...extra,
  }
}

type Answer = Response | ((init?: RequestInit) => Response | Promise<Response>)

export type BffOptions = {
  /** GET /api/session: a role, or the reply. Default 'admin'. */
  session?: 'admin' | 'user' | 'signed-out' | Answer
  /** POST /api/session: the reply. Default 200 for an administrator. */
  signIn?: Answer
  /** DELETE /api/session: the reply. Default 204. */
  signOut?: Answer
  /** GET /api/version: the reply. Default 200 "2.99.0". */
  version?: Answer
  /** GET /api/instances: the instances, or the reply. Default INSTANCES. */
  instances?: Instance[] | Answer
  /** GET /api/presets: the presets, or the reply. Default PRESETS. */
  presets?: Preset[] | Answer
  /**
   * PUT /api/presets/{id}/enabled: the reply to the n-th switch (from 1),
   * given the parsed body and the decoded id. Default 200 with the body's
   * `enabled` echoed and `running: false`.
   */
  presetSwitch?: (
    body: unknown,
    id: string,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /** GET /api/instances/{id}/server: the reply per id. Default serverDoc(). */
  server?: (id: string, init?: RequestInit) => Response | Promise<Response>
  /**
   * PUT /api/instances/{id}/server: the reply to the n-th save (from 1), given
   * the parsed body. Default 200 {"revision": "r-2", "running": false}.
   */
  save?: (
    body: unknown,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * POST /api/instances/{id}/start, /stop or /restart: the reply to the n-th
   * run action (from 1, all three counted together). Default 204.
   */
  run?: (
    action: 'start' | 'stop' | 'restart',
    id: string,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * GET and PUT /api/instances/{id}/boot. `read`: the reply to the n-th GET
   * (from 1), default 200 {"enabled": false}. `write`: the reply to the n-th
   * PUT (from 1), given the parsed body, default 200 with the body's
   * `enabled` echoed.
   */
  boot?: {
    read?: (
      id: string,
      n: number,
      init?: RequestInit,
    ) => Response | Promise<Response>
    write?: (
      body: unknown,
      id: string,
      n: number,
      init?: RequestInit,
    ) => Response | Promise<Response>
  }
}

export type Call = { method: string; path: string; body: unknown }

function answer(a: Answer, init?: RequestInit): Promise<Response> {
  return Promise.resolve(typeof a === 'function' ? a(init) : a)
}

/** Stubs fetch with a fake bff; returns the calls it saw. */
export function fakeBff(options: BffOptions = {}) {
  const calls: Call[] = []
  // What GET /api/instances answers from now on (setInstances changes it).
  let instances = options.instances
  const fetch = vi.fn<typeof globalThis.fetch>((input, init) => {
    const url =
      input instanceof Request
        ? input.url
        : String(input instanceof URL ? input.href : input)
    const path = url.replace(/^https?:\/\/[^/]+/, '')
    const method = (
      init?.method ?? (input instanceof Request ? input.method : 'GET')
    ).toUpperCase()
    let body: unknown = undefined
    if (typeof init?.body === 'string') {
      try {
        body = JSON.parse(init.body)
      } catch {
        body = init.body
      }
    }
    calls.push({ method, path, body })
    const route = `${method} ${path}`
    if (route === 'GET /api/session') {
      const s = options.session ?? 'admin'
      if (s === 'admin')
        return Promise.resolve(
          reply(200, { user: 'alice', canConfigure: true }),
        )
      if (s === 'user')
        return Promise.resolve(reply(200, { user: 'bob', canConfigure: false }))
      if (s === 'signed-out')
        return Promise.resolve(
          reply(401, { key: 'session-required', args: [] }),
        )
      return answer(s, init)
    }
    if (route === 'POST /api/session')
      return answer(
        options.signIn ??
          (() => reply(200, { user: 'alice', canConfigure: true })),
        init,
      )
    if (route === 'DELETE /api/session')
      return answer(options.signOut ?? (() => reply(204)), init)
    if (route === 'GET /api/version')
      return answer(
        options.version ?? (() => reply(200, { version: '2.99.0' })),
        init,
      )
    if (route === 'GET /api/instances') {
      const i = instances ?? INSTANCES
      if (Array.isArray(i)) return Promise.resolve(reply(200, i))
      return answer(i, init)
    }
    if (route === 'GET /api/presets') {
      const p = options.presets ?? PRESETS
      if (Array.isArray(p)) return Promise.resolve(reply(200, p))
      return answer(p, init)
    }
    const presetSwitch = /^PUT \/api\/presets\/([^/]+)\/enabled$/.exec(route)
    if (presetSwitch) {
      const id = decodeURIComponent(presetSwitch[1]!)
      const n = calls.filter(
        (c) =>
          c.method === 'PUT' && /^\/api\/presets\/[^/]+\/enabled$/.test(c.path),
      ).length
      const enabled = (body as { enabled?: unknown } | undefined)?.enabled
      return Promise.resolve(
        options.presetSwitch
          ? options.presetSwitch(body, id, n, init)
          : reply(200, { enabled, running: false }),
      )
    }
    const server = /^GET \/api\/instances\/([^/]+)\/server$/.exec(route)
    if (server) {
      const id = decodeURIComponent(server[1]!)
      return Promise.resolve(
        options.server ? options.server(id, init) : reply(200, serverDoc()),
      )
    }
    if (/^PUT \/api\/instances\/[^/]+\/server$/.test(route)) {
      const n = calls.filter(
        (c) => c.method === 'PUT' && c.path.endsWith('/server'),
      ).length
      return Promise.resolve(
        options.save
          ? options.save(body, n, init)
          : reply(200, { revision: 'r-2', running: false }),
      )
    }
    const run = /^POST \/api\/instances\/([^/]+)\/(start|stop|restart)$/.exec(
      route,
    )
    if (run) {
      const n = calls.filter((c) =>
        /^\/api\/instances\/[^/]+\/(start|stop|restart)$/.test(c.path),
      ).length
      const action = run[2] as 'start' | 'stop' | 'restart'
      return Promise.resolve(
        options.run
          ? options.run(action, decodeURIComponent(run[1]!), n, init)
          : reply(204),
      )
    }
    const boot = /^(GET|PUT) \/api\/instances\/([^/]+)\/boot$/.exec(route)
    if (boot) {
      const id = decodeURIComponent(boot[2]!)
      const n = calls.filter(
        (c) => c.method === boot[1] && c.path.endsWith('/boot'),
      ).length
      if (boot[1] === 'GET')
        return Promise.resolve(
          options.boot?.read
            ? options.boot.read(id, n, init)
            : reply(200, { enabled: false }),
        )
      const enabled = (body as { enabled?: unknown } | undefined)?.enabled
      return Promise.resolve(
        options.boot?.write
          ? options.boot.write(body, id, n, init)
          : reply(200, { enabled }),
      )
    }
    return Promise.reject(new TypeError(`unexpected ${route}`))
  })
  vi.stubGlobal('fetch', fetch)
  return {
    fetch,
    calls,
    /** The calls to one route. */
    to(method: string, path: string): Call[] {
      return calls.filter((c) => c.method === method && c.path === path)
    },
    /** What GET /api/instances answers from the next read on. */
    setInstances(i: Instance[] | Answer) {
      instances = i
    },
  }
}

/** Puts the page on `hash` before it renders (no hashchange event). */
export function startAt(hash: string) {
  window.history.replaceState(null, '', `/${hash}`)
}

/** Moves the rendered page to `hash`, as a click on a link does. */
export function navigate(hash: string) {
  window.location.hash = hash
}

/** True when the element is not shown: absent, or under `hidden`. */
export function isHidden(el: Element | null): boolean {
  if (el === null || !el.isConnected) return true
  return el.closest('[hidden]') !== null
}

/** True when `a` comes before `b` in the document. */
export function before(a: Node, b: Node): boolean {
  return (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0
}

/**
 * jsdom has no HTMLDialogElement.prototype.showModal or close. This stands in
 * for the browser's: showModal sets `open` (so the dialog is in the
 * accessibility tree and found by role) and marks it modal; close removes
 * `open` and fires `close`; Escape inside a modal dialog fires a cancelable
 * `cancel`, then closes it unless the cancel was prevented. It does NOT move
 * the focus back on close (browsers do): the page must do it itself.
 * Returns the restore function and the dialogs showModal opened.
 */
export function stubDialog(): {
  restore: () => void
  opened: HTMLDialogElement[]
} {
  const proto = HTMLDialogElement.prototype as unknown as Record<
    string,
    unknown
  >
  const saved = { showModal: proto.showModal, close: proto.close }
  const opened: HTMLDialogElement[] = []
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape') return
    const dialog = e.currentTarget as HTMLDialogElement
    const cancel = new Event('cancel', { cancelable: true })
    if (dialog.dispatchEvent(cancel)) dialog.close()
  }
  proto.showModal = function (this: HTMLDialogElement) {
    if (this.hasAttribute('open')) {
      throw new DOMException('already open', 'InvalidStateError')
    }
    this.setAttribute('open', '')
    this.dataset.testModal = 'true'
    this.addEventListener('keydown', onEscape)
    opened.push(this)
  }
  proto.close = function (this: HTMLDialogElement, value?: string) {
    if (!this.hasAttribute('open')) return
    this.removeAttribute('open')
    delete this.dataset.testModal
    this.removeEventListener('keydown', onEscape)
    if (value !== undefined) this.returnValue = value
    this.dispatchEvent(new Event('close'))
  }
  return {
    opened,
    restore() {
      proto.showModal = saved.showModal
      proto.close = saved.close
    },
  }
}
