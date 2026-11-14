# pai-bff and the web UI

`pai-bff` (package and binary `pai-bff`, over `pai-core`) is the backend-for-frontend: one local
HTTP service that serves the web UI built from `fe/` and the API it calls. Today it answers the
framework version and the manager's instances with their state, and serves a page that shows both,
read-only (`PLAN.md` step 10).

**It runs as a service on Linux and Windows, never on macOS.** `xtask build` compiles it into
`build/<os>/cargo-target/release/`; the packaging is in
[docs/build-packaging.md](../../docs/build-packaging.md).

- Linux: the package `private-ai-framework-bff` (which requires `private-ai-framework-core`, not the
  engine) puts it in `/usr/libexec/private-ai/pai-bff` with `private-ai-bff.service`, enabled and
  started at install, stopped before an upgrade and started again after, stopped on removal. It runs as the static system user
  `private-ai-bff` (no home, no shell; created by the package's own sysusers.d entry), hardened with
  the sandbox `DynamicUser=` would imply written out: a `DynamicUser=` cannot work, because dbus-daemon
  drops a dynamic user's connection at `Hello` (on Ubuntu 26.04, dbus-daemon 1.16.2, `/api/instances`
  answered 503; sources in the unit's test in `xtask/src/linux/system_files.rs`). Checked on the real
  install of 2.99.1 on Ubuntu 26.04 (2026-11-14, by the user): the service active, `/api/version`
  answered, `/api/instances` `[]` with 200, and 503 `unavailable` with the manager stopped.
- Windows: the framework's MSI installs the service `private-ai-bff` (`pai-bff.exe service`, automatic start,
  `NT SERVICE\private-ai-bff`, restarted after a failure). Its `Error:` line goes nowhere there: by
  the code, a bad `bff.ini` or a bind failure shows only in the service manager's state and exit code.
  Type-checked only, not yet run on Windows; the open checks are in the test plan of
  [installer/AGENTS.md](../../installer/AGENTS.md).

**It stops gracefully on SIGTERM (how systemd stops it) and Ctrl-C**: it stops accepting, lets the
open requests finish and exits 0. Under the Windows service manager the stop control does the same.

## Running it

**The address is `[Bff] Listen` in `bff.ini`, in the system config directory**
(`paths::system_config_dir`: `/etc/private-ai/bff.ini` on Linux and macOS,
`%ProgramData%\private-ai\config\bff.ini` on Windows, `<root>/system-config/bff.ini` under
`PRIVATE_AI_DATA_ROOT`). A missing file or key means `127.0.0.1:8590`.

```ini
[Bff]
Listen = 127.0.0.1:8590
```

- The value is an `ip:port` address: a host name (`localhost:8590`) is refused, nothing is resolved.
- Only a loopback address is accepted. LAN access waits for authentication (a later slice).

| Exit code | When |
| --- | --- |
| 0 | `pai-bff --version`, which prints `pai-bff <version>`; a stop by SIGTERM, Ctrl-C or the service manager |
| 2 | Any other argument, or anything after `service` (usage error) |
| 1 | A refused `Listen` value, an unreadable `bff.ini`, a bind failure or a serving error; `pai-bff service` outside the Windows service manager |

Every failure prints one `Error: ...` line on stderr naming the value, the file or the address. The
texts are the `bff-*` keys of `crates/core/strings/cli.toml`, found by the installed program in
`/usr/share/private-ai/strings/` on Linux (`paths::installed_share_candidates`). `pai-bff service`
hands the process to the Windows service dispatcher, which runs the same server. It serves on a current-thread tokio
runtime (axum).

| Route | Answer |
| --- | --- |
| `GET /api/version` | `{"version": "<framework version>"}` |
| `GET /api/instances` | `[{"id": "<id>", "state": "<state>"}]` from `private-ai-manager` ([below](#the-instances-from-the-manager)) |
| `GET /` | The embedded `index.html` |
| `GET /assets/{*path}` | The embedded file at `assets/<path>` |

Other methods on these routes are 405, any other path 404. Only a path in the embedded table
matches, so `..` reaches nothing else. Content types go by extension: HTML, JavaScript
(`text/javascript`), CSS, anything else `application/octet-stream`.

Checked by hand on Linux from the debug build (2026-11-14): `--version` exits 0, an unknown argument
2, `0.0.0.0:8590` and `localhost:8590` exit 1; on `127.0.0.1` it answered `/api/version`, `/` as
`text/html`, 405 to a `POST` and 404 elsewhere. Under `PRIVATE_AI_DATA_ROOT` `/api/instances`
answered 503 `{"error":"unavailable"}` and a `POST` to it 405.

## The instances from the manager

`GET /api/instances` asks `private-ai-manager` for `ListInstances`, then `Status` of each instance:
the two calls the manager answers without authorization, and the only ones the bff makes
(`crates/bff/src/lib.rs`).

| Status | Body | When |
| --- | --- | --- |
| 200 | `[{"id", "state"}]` in the manager's order, `[]` when there is none | Every call answered; an instance removed between the two calls is left out |
| 503 | `{"error": "unavailable"}` | No manager to reach (none running, macOS), or no full answer within 5 s |
| 502 | `{"error": "not-authorized"}` | The manager refused a call |
| 502 | `{"error": "failed"}` | Any other failure |

- **The state is the manager's name** (`InstanceState::name()` in `pai-core`: `active`, `inactive`,
  `activating`, `deactivating`, `failed`, or an unknown name passed on as is). The error body is only
  the key, never the manager's detail.
- **A new connection per request**, so a manager started after the bff is seen at the next request.
  The connect and the calls run on a blocking thread under one 5 s timeout that covers the connect.
- **Under `PRIVATE_AI_DATA_ROOT` it dials nothing** and answers 503 `unavailable`, so a test never
  reaches the real manager.
- **Reaching it**: on Linux the system bus (`org.privateai.Manager1`), whose policy lets every user
  send to the manager; neither call asks polkit, and with no session the bff's user `private-ai-bff` gets no
  `control` (`auth_admin`). On Windows the pipe `\\.\pipe\private-ai-manager`, whose SDDL gives
  `NT SERVICE\private-ai-bff` the interactive client's rights; its token is neither interactive nor an
  administrator's, so the manager refuses it `control` and `configure`
  ([installer/AGENTS.md](../../installer/AGENTS.md)). Type-checked only, not yet run on Windows.

## The web UI (`fe/`)

React + Vite in TypeScript, exact versions in `fe/package.json` and a committed
`package-lock.json`. One page (`fe/src/App.tsx`) shows, each read once when it opens and never the
reply's body:

- the version (`fe/src/Version.tsx`, from `/api/version`), or a fixed message on any failure;
- the instances (`fe/src/Instances.tsx`, from `/api/instances`): a table of instance and state (the
  five known states as Running, Stopped, Starting, Stopping, Failed; another as `Unknown (<name>)`),
  a message when there is none, and one message per error key: `unavailable` (the manager is not
  running or does not answer), `not-authorized` (it refused the web service), anything else,
  a malformed reply included, as "could not be read".

- **Text goes in `fe/src/strings/en_US.json`**, linked by key through `t(key, ...args)`
  (`fe/src/strings.ts`), with `pai-core`'s rules: `{N}` placeholders filled by position, a missing
  key shown as itself.
- **Tests**: Vitest and Testing Library in jsdom, `npm --prefix fe test` from the repo root
  (`npm --prefix fe ci` first when `fe/node_modules` is missing).
- **Checks**, none of which writes a file: `npm --prefix fe run typecheck` (`tsc` over the app,
  `vite.config.ts` and, through `fe/tsconfig.test.json`, the tests, which `npm run build` does not
  type-check), `npm --prefix fe run lint` (ESLint, `fe/eslint.config.js`, no warning allowed) and
  `npm --prefix fe run format:check` (Prettier, `fe/.prettierrc.json`). TypeScript stays on 6.0:
  typescript-eslint does not load 7 yet ("Later, not planned yet" in [PLAN.md](../../PLAN.md)).
- **Built by `xtask build`**, embedded in the binary by `crates/bff/build.rs`: which directory it
  embeds and when it notices a new `fe/dist` are in
  ["The web UI and pai-bff"](../../docs/build-compiling.md#the-web-ui-and-pai-bff).

## Tests

`cargo test -p pai-bff` needs no node: without `fe/dist` it embeds the fixture
`crates/bff/fe-fixture/`. `tests/fe.rs` checks that the routes serve, byte for byte, the directory
the build embedded (`PAI_BFF_FE_DIR`); `tests/instances.rs` drives `/api/instances` against a fake
manager (`router_with`, a short timeout); `tests/pai_bff_binary.rs` runs the binary with
`PRIVATE_AI_DATA_ROOT` on the child and an ephemeral port, SIGTERM and SIGINT included; `src/tests/deps_lint.rs` keeps the GUI
packages (Slint) out of its dependency tree. It is in the Windows GNU type check ("Commands" in
[AGENTS.md](../../AGENTS.md#commands)); nothing of it has run on Windows.
