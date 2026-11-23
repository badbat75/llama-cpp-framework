// Test helpers for the web UI: a fake bff behind a mocked fetch (no network),
// the English texts of the design brief, and the hash route of the page.
// The shapes are those of fe/briefs/api-contract.md.
import { test, vi } from 'vitest'

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
  'theme-dark': 'Dark theme',
  'nav-overview': 'Overview',
  // Brief 20 section 2 (slice N1): "Nodes" replaces "Instances" (nav-instances
  // is removed with that slice), the host entry falls back to "This node", and
  // an instance link carries its server profile twice, once for the reader and
  // once for the eye.
  'nav-nodes': 'Nodes',
  'nav-this-node': 'This node',
  'nav-instance-profile': ', server profile {0}',
  'nav-instance-profile-shown': '· {0}',
  loading: 'Loading…',
  'not-found': 'There is no page at this address.',
  'not-found-home': 'Go to the overview',
  'session-sign-in': 'Sign in',
  'session-sign-out': 'Sign out',
  'session-signed-in-as': 'Signed in as {0}',
  'signin-title': 'Sign in',
  'signin-intro':
    'Use the user name and password of your account on the computer that runs Private AI Framework. Only members of its pai group can sign in.',
  'signin-required': 'Sign in to see this page.',
  'signin-session-ended': 'Your session has ended. Sign in again to continue.',
  'signin-username': 'User name',
  'signin-password': 'Password',
  'signin-submit': 'Sign in',
  'signin-busy': 'Signing in…',
  'signin-cancel': 'Cancel',
  'signin-empty-username': 'Enter your user name.',
  'signin-empty-password': 'Enter your password.',
  'signin-failed':
    'Signing in failed. Try again; if it keeps failing, the web service may not be running.',
  'signin-insecure':
    'This connection is not encrypted, so your password is sent over the network as plain text. To keep it private, sign in on the computer that runs Private AI Framework, or only over a network you trust.',
  'api-session-bad-credentials': 'The user name or password is not correct.',
  'api-session-too-many-attempts':
    'Too many attempts. Try again in {0} seconds.',
  'api-session-unavailable':
    'Signing in is not available on this server right now.',
  'api-session-not-administrator':
    'Only members of the pai group can sign in, and this account is not one. Ask whoever administers this computer to add it.',
  'session-revoked':
    'You have been signed out: this account is no longer a member of the pai group. Ask whoever administers this computer to add it again.',
  'overview-title': 'Overview',
  'instance-engine': '{0} instance',
  'instance-state': 'State: {0}',
  'instance-not-found': 'There is no instance named {0}.',
  'server-title': 'Server',
  // Brief 16 slice 17a: the Server form moves from the instance page to a
  // server profile's page, and its intro names the profile's file; the key is
  // new and `server-intro` is gone with the same slice (17a.12).
  'server-profile-intro':
    'llama-server settings of the instance that runs this server profile, shared by every model it serves ({0}). Per-model settings belong to the model profiles.',
  'server-loading': 'Loading the server settings…',
  'server-load-failed': 'The server settings could not be read.',
  // Brief 16 slice 17a: the server profiles list and a profile's page.
  'nav-server-profiles': 'Server profiles',
  'server-profiles-title': 'Server profiles',
  'server-profiles-intro':
    'Each server profile holds the llama-server settings of one instance: its port, its addresses, its tags and the rest. An instance runs one server profile, and a server profile serves one instance at a time.',
  'server-profiles-loading': 'Loading the server profiles…',
  'server-profiles-failed': 'The server profiles could not be read.',
  'server-profiles-empty': 'There are no server profiles yet.',
  'server-profiles-column-name': 'Server profile',
  'server-profiles-column-port': 'Port',
  'server-profiles-column-tags': 'Tags',
  'server-profiles-column-used-by': 'Used by',
  'server-profiles-unused': 'No instance',
  'server-profile-back': 'All server profiles',
  'server-profile-used-by': 'Used by',
  'server-profile-not-found': 'There is no server profile named {0}.',
  'server-folder-unused-default':
    'unset: the instance that runs this profile uses its own folder',
  'instance-server-title': 'Server profile',
  'instance-server-none': 'None',
  // Brief 16 slice 17b: the create form on the pool list, and the delete of a
  // profile no instance runs on its page. `1a strings` in App.test.tsx pins
  // these against strings/en_US.json, so the sheet gains exactly 17b.10.
  'server-profiles-new': 'New server profile',
  'server-profiles-new-title': 'New server profile',
  'server-profile-name': 'Name',
  'server-profile-name-hint':
    '1 to 32 characters: a-z, 0-9 and -, starting with a letter or a digit. It names the file server@<name>.ini and cannot be changed later.',
  'server-profile-port-hint':
    'The network port llama-server listens on. Filled in with the first port from 9931 up that no server profile uses; change it if you like. Two instances cannot run on the same address and port.',
  'create-button': 'Create',
  'create-cancel': 'Cancel',
  'create-creating': 'Creating…',
  'field-error-required': 'Enter a value.',
  'create-unknown':
    'The manager did not answer in time, so it may or may not have been created. The list shows it if it was.',
  'server-profile-delete': 'Delete server profile',
  'server-profile-delete-question':
    'Delete the server profile {0}? Its settings file is removed, with any unsaved changes on this page. This cannot be undone.',
  'confirm-delete': 'Delete',
  'confirm-cancel': 'Cancel',
  'confirm-deleting': 'Deleting…',
  'server-profile-deleted': 'The server profile {0} was deleted.',
  'api-client-server-profile-exists':
    'The server profile {0} is already there.',
  'api-client-server-profile-in-use':
    'The server profile {0} is already used by instance {1}.',
  'api-client-no-server-profile': 'There is no server profile of that name.',
  'api-server-profile-bad-id':
    "invalid server profile id `{0}`: use lowercase letters, digits and '-', starting with a letter or digit, up to 32 characters",
  'api-unknown-engine':
    'unknown engine `{0}`: this manager serves the engine llama.cpp',
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
  // Brief 17, U17a.1: the Overview has a loading text of its own, its status
  // while GET /api/instances is pending.
  'instances-loading': 'Loading the instances…',
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
  // Sub-slice 2h.10 (fe/briefs/02h-model-profile-save.md, criterion 198a): the alert
  // for a refusal on a field that cannot take focus.
  'save-refused-at': 'Not saved: {0}. The field is {1}, on the card {2}.',
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
  'field-error-whole-number': 'Enter a whole number, without a decimal point.',
  'field-error-gpu-layers': 'Enter a whole number, auto or all.',
  'field-error-decimal':
    'Enter a number, such as 0.7 (a decimal comma also works).',
  'field-error-comment-markers':
    'Remove the ; and # characters: the INI file would cut the value there.',
  'field-error-fit-target':
    'Enter MiB as whole numbers: one number, or one per device separated by commas, such as 2048,1024.',
  'field-error-json-object':
    'Enter one JSON object in braces, such as {"enable_thinking": false}.',
  'api-not-whole-number-in-range':
    '{0} = `{1}` is not a whole number from {2} to {3}',
  'api-comment-markers':
    "{0} contains ';' or '#', which starts a comment in the INI format and cuts the value short; use a value without those characters",
  'api-load-on-startup-over-max':
    '{0} enabled model profiles would load on startup, but the server loads at most {1} models at once (ModelsMax) and would not start. Turn Load on startup off on another one, or raise ModelsMax',
  'api-load-on-startup-unlimited':
    '{0} enabled model profile(s) would load on startup with ModelsMax = 0, which llama-server rejects at startup (it reads 0 as a limit there, not as unlimited). Set ModelsMax to at least {0}, or turn Load on startup off',
  // The manager can name the web service itself as the holder of the address,
  // so the sentence names only the holder ({0}).
  'api-client-address-in-use': '{0} already uses that address and port.',
  'api-client-unresolvable-host':
    'The host name {0} does not resolve to an address.',
  // Slice S3 (section 4e): a save of a hand-edited file that binds 0.0.0.0
  // with other addresses. The English is the en_US.toml line, verbatim.
  'api-bind-all-with-others':
    'Hostname lists 0.0.0.0 with other addresses; 0.0.0.0 already covers every address, so bind it alone.',
  'api-server-changed-elsewhere':
    'These settings changed elsewhere, or the web service restarted, after you opened them. Reload to see them; your edits here will be lost.',
  // Sub-slice 2h.2 (fe/briefs/02h-model-profile-save.md, criterion 145): a model profile
  // save answered 409 model-profile-changed-elsewhere. The English is the brief's,
  // verbatim.
  'api-model-profile-changed-elsewhere':
    'This model profile changed elsewhere (edited, or switched on or off), or the web service restarted, after you opened it. Reload to see it as it is now; your edits here will be lost.',
  'api-other': 'The request was refused ({0}).',
  // Sub-slice 2h.2 (fe/briefs/02h-model-profile-save.md, the table of criterion 150):
  // the bff's 422 keys for a model profile. Each English is the en_US.toml line the
  // criterion names, verbatim (no closing full stop, as every copied check).
  'api-not-a-number': '{0} = `{1}` is not a number',
  'api-not-one-of': '{0} = `{1}` is not one of: {2}',
  'api-not-json-object':
    '{0} is not a JSON object (a list of "key": value pairs in braces)',
  'api-min-above-max':
    '{0} = {1} is above {2} = {3}: the minimum cannot exceed the maximum',
  'api-fit-target-format':
    'fit-target `{0}`: {1} as whole numbers, one value or a comma-separated list per device',
  'api-moe-cache-with-tensor-split':
    "moe-cache-mib with split-mode tensor: llama.cpp's MoE cache does not support tensor parallelism and the model would not load. Clear the MoE cache or pick another split mode",
  'api-model-profile-no-model':
    '`{0}` names no model file: every model profile needs a `model` path',
  'api-override-no-pattern': 'tensor override {0} has an empty pattern',
  'api-override-no-device':
    'tensor override {0} (`{1}`) names no device: a rule is `<pattern>=<device>`, and llama.cpp splits it at the first `=`',
  'api-invalid-model-profile-id':
    "invalid model profile id `{0}`: use letters, digits, '.', '-', '_'",
  'api-model-profile-name-taken': 'a model profile named `{0}` already exists',
  // Brief 11 (fe/briefs/11-resource-refusals.md, slice M1): the two resource
  // refusals of a model profile. Each English is section 2 and section 7's,
  // verbatim, backticks included (the GUI's sentence plus the web's next step).
  'api-model-profile-over-memory':
    '`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type (cards Resource / context and KV cache).',
  'api-model-profile-model-missing':
    '`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or change the Model file in pai-config there.',
  // Brief 12 (fe/briefs/12-instance-tags.md, section 2): the bff's refusal of
  // tags. The English is crates/core/strings/en_US.toml's, verbatim.
  'api-server-bad-tags':
    '{0}: the tags {1} are not a list of at most {2} tags, each of {3} to {4} characters of a-z, 0-9 and -, none twice',
  'server-edits-dropped':
    'Your unsaved changes were dropped: you signed in with a different account. The form shows the saved settings.',
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
  // Slice S2 (section 4d): a Stop or Restart that saves snapshots for minutes.
  'run-timeout-stop':
    'llama-server did not reach the expected state within six minutes. The state above keeps updating.',
  'request-failed': 'The request did not go through. Try again.',
  'session-ended-retry': 'Your session has ended. Sign in again, then retry.',
  'api-client-service-failed':
    "The service manager reported an error. The instance's log on the server says why.",
  // Slice 1c (section 5.7): start at boot.
  'boot-label': 'Start at boot',
  // Slice H6b (08-hearth-h6-overview.md, H6b.5): the two words of the boot row
  // of a tile. New keys; the label of the pair is `boot-label`, reused.
  'instances-boot-on': 'On',
  'instances-boot-off': 'Off',
  'boot-toggle': 'start llama-server when the system starts',
  'boot-hint':
    "Starts llama-server when the system starts, before anyone logs in: the instance's service is enabled (Linux: systemctl enable llama.cpp@default; Windows: its start type is Automatic).",
  'boot-on': 'llama-server will start when the system starts.',
  'boot-off': 'llama-server will no longer start when the system starts.',
  'boot-load-failed': 'Whether llama-server starts at boot could not be read.',
  // Brief 12 (fe/briefs/12-instance-tags.md, slice T1): the instance's port on
  // its page, and its own tags. Each English is section 2's, verbatim; the tile's
  // Port row reuses server-port, its Tags row instance-tags.
  'instance-port': 'Port {0}',
  'instance-tags': 'Tags',
  'instance-tags-own': 'Instance tags',
  // Brief 15 (fe/briefs/15-tag-chips.md, slice C1): the tags as chips. Each
  // English is section 2's, verbatim. instance-tags-hint is section 2's new text,
  // and instance-tags-save, instance-tags-saved and instance-tags-profile are
  // gone with the text input they served.
  'instance-tags-hint':
    'Labels you give this instance to tell it apart; they change nothing in how it runs or which requests reach it. Add one with the + button, using a-z, 0-9 and -; each change is saved at once. The tags of its server profile are shown below and are not changed here.',
  'instance-tags-from-profile': 'From the server profile',
  'instance-tag-add': 'Add tag',
  'instance-tag-new': 'New tag',
  'instance-tag-confirm': 'Add',
  'instance-tag-cancel': 'Cancel',
  'instance-tag-remove': 'Remove tag {0}',
  'instance-tag-shown': '{0} is already a tag of this instance.',
  'instance-tags-saving': 'Saving tags…',
  'instance-tag-added': 'Tag {0} added.',
  'instance-tag-removed': 'Tag {0} removed.',
  // Brief 15 C2 (fe/briefs/15-tag-chips.md, section 2): the name of the list the
  // "New tag" box points at with aria-controls.
  'instance-tag-suggestions': 'Used tags',
  // Slice 1c (section 5.7, 4.8): a save that timed out.
  'save-timeout':
    'Not confirmed: the service did not answer in time, so the settings may or may not have been saved. Your changes are kept; save again to be sure.',
  // Slice 2a (fe/briefs/02-models.md, section 2.7): the model profile list.
  // Slice 05b (fe/briefs/05b-profiles-nav.md, 5b.11): the nav's Profiles group.
  // 'nav-models' and 'models-title' are gone: the nav entry, the h1 and the
  // document title all say 'Model profiles'. 'Server profiles' has no key here,
  // because 05b does not show it (5b.2); 05c adds it with its page.
  'nav-profiles': 'Profiles',
  'nav-model-profiles': 'Model profiles',
  'model-profiles-title': 'Model profiles',
  'model-profiles-intro':
    "Each model profile is one model with its settings. Every instance of the profile's engine can serve it.",
  'model-profiles-column-name': 'Model profile',
  'model-profiles-column-engine': 'Engine',
  'model-profiles-column-status': 'Status',
  // Slice U17c (fe/briefs/17-ux-pass.md): the sortable headers' announcements, on
  // the filter's own status line.
  'model-profiles-sorted-asc': 'Sorted by {0}, A to Z.',
  'model-profiles-sorted-desc': 'Sorted by {0}, Z to A.',
  'model-profiles-sorted-file': 'In file order.',
  'model-profiles-status-on': 'On',
  'model-profiles-status-off': 'Off',
  'model-profiles-engine-not-available': '{0} (not available)',
  'model-profiles-loading': 'Loading the model profiles…',
  'model-profiles-empty': 'No model profiles yet.',
  'model-profiles-load-failed': 'The model profiles could not be read.',
  'model-profiles-off-note':
    'A model profile switched off is kept, but its engine does not serve it and OpenCode does not offer it.',
  // Slice 2b (fe/briefs/02-models.md, section 2b.9): the list's filter.
  'model-profiles-filter-label': 'Filter model profiles',
  'model-profiles-filter-hint':
    'Matches part of the model profile name or of its model file path. Case does not matter.',
  'model-profiles-filter-clear': 'Clear filter',
  'model-profiles-filter-count': 'Model profiles shown: {0} of {1}.',
  'model-profiles-no-match': 'No model profiles match the filter.',
  // Slice 2c (fe/briefs/02-models.md, section 2c.8): the on/off switch.
  'model-profiles-switched-on': 'Switched {0} on.',
  'model-profiles-switched-off': 'Switched {0} off.',
  'model-profiles-switched-on-restart':
    'Switched {0} on: restart the running instances of {1} to apply.',
  'model-profiles-switched-off-restart':
    'Switched {0} off: restart the running instances of {1} to apply.',
  'api-no-model-profile-named':
    'There is no model profile named {0} any more: it was renamed or deleted elsewhere. Open the list of all model profiles again to see them as they are.',
  // Slice 2d (fe/briefs/02-models.md, section 2d.10): the model profile page, the
  // keys the criteria of 2d.1 to 2d.4 use, and those of the content (2d.5 to 2d.8).
  'model-profile-back': 'All model profiles',
  'model-profile-loading': 'Loading the model profile…',
  'model-profile-load-failed': 'The model profile could not be read.',
  'model-profile-not-found': 'There is no model profile named {0}.',
  'model-profile-off':
    'This model profile is switched off: it is kept, but its engine does not serve it. To serve it, switch it on in the list of all model profiles.',
  'model-profile-settings-title': 'Model profile settings',
  'model-profile-engine': 'Engine',
  'model-profile-engine-unavailable':
    '{0} is not available on the server, so no instance can serve this model profile until it is installed again.',
  'model-profile-assets': 'Assets',
  'model-profile-model-file': 'Model file',
  'model-profile-project-url': 'Project URL',
  'model-profile-draft-model': 'Draft model',
  'model-profile-drafter': 'Drafter',
  'model-profile-ngram-drafters': 'N-gram drafters',
  'model-profile-value-none': '(none)',
  // Sub-slice 2h.10 (fe/briefs/02h-model-profile-save.md, criteria 195, 196): the
  // Assets card intro, and the name of the section jump list.
  'model-profile-assets-read-only':
    'The files, Project URL and drafters of a model profile cannot be changed here yet. Change them in pai-config on the server machine.',
  'model-profile-jump-label': 'On this page',
  // Slice 2e.1 (fe/briefs/02e-model-profile-cards.md, criterion 95): the Resource /
  // context and Embeddings cards, and the pooling options the shared texts of
  // 1.3 lack. The hints are the GUI's tooltips (en_US.toml), unchanged except
  // model-profile-context-size-hint, whose web text criterion 92 gives.
  // Slice 2h.1's Notice model-profile-partly-editable, and the read-only Notice
  // model-profile-read-only it replaced, are both out (2h.10's 194).
  'model-profile-resource-context': 'Resource / context',
  'model-profile-context-size': 'Context size',
  'model-profile-context-size-hint':
    "How many tokens of conversation the model can hold. Bigger = longer history, more VRAM for the KV cache. It can be divided among requests: see Parallel slots.\nDefault: 0 (llama.cpp's own), meaning the context the model was trained with.\nFlag: --ctx-size",
  'model-profile-parallel-slots': 'Parallel slots',
  'model-profile-parallel-slots-hint':
    "How many requests can run at once. A number you set DIVIDES the context among them: 2 leaves each request half of Context size.\nDefault: auto (llama-server's own), 4 slots sharing one KV cache, so one request can use the WHOLE context. Leave it at default to give one request the model's full context.\nFlag: --parallel",
  'model-profile-unified-kv': 'Unified KV',
  'model-profile-unified-kv-hint':
    'One KV cache shared by all parallel slots, so any request can grow into the WHOLE context instead of its own share.\nOn pays off when requests are uneven (one long, others short). Off gives each slot a fixed share, so one long request cannot starve the others.\nDefault: off with Parallel slots set. With Parallel slots at default, llama-server turns it on anyway, so this is disabled.\nFlags: --kv-unified, --no-kv-unified',
  'model-profile-per-slot-context': 'Per-slot context',
  'model-profile-per-slot-context-hint':
    "Limits every slot to this many tokens. Works with Unified KV on or off.\nWith Context size at default it also sizes the KV cache as Parallel slots × this value, instead of the model's trained context: 4 slots at 32768 allocate 128k tokens.\nDefault: no limit.\nFlag: --kv-unified-per-slot",
  'model-profile-batch-size': 'Batch size',
  'model-profile-batch-size-hint':
    "How many tokens of a prompt are handled per step. Higher speeds up prefill (reading the prompt) and uses more VRAM.\nDefault: 2048 (llama.cpp's own).\nFlag: --batch-size",
  'model-profile-u-batch-size': 'U-Batch size',
  'model-profile-u-batch-size-hint':
    "How many tokens are actually computed at once. Must not exceed Batch size.\nDefault: 512 (llama.cpp's own).\nFlag: --ubatch-size",
  'model-profile-embeddings': 'Embeddings',
  'model-profile-embedding-mode': 'Embedding mode',
  'model-profile-embedding-mode-hint':
    'Serves this model as an embedding model, turning on /v1/embeddings (without it, llama-server answers 501 "This server does not support embeddings"). Only for dedicated embedding models.\nEvery input must fit in one batch: set Batch size and U-Batch size above to at least your longest text, or the request fails with "input is too large" (U-Batch size defaults to 512 tokens).\nDefault: off.\nFlag: --embeddings',
  'model-profile-pooling': 'Pooling',
  'model-profile-pooling-hint':
    "How a text's token vectors become ONE embedding. Only used in embedding mode.\nDefault: the model's own, read from its GGUF.\nnone = one vector per token; mean = their average; cls = the first token's; last = the last token's (Qwen3-Embedding); rank = the reranker head.\nA wrong choice gives no error, only vectors of the right size that search worse.\nFlag: --pooling",
  'option-mean': 'mean',
  'option-last': 'last',
  'option-rank': 'rank',
  // Slice 2e.2 (fe/briefs/02e-model-profile-cards.md, criterion 100): the KV cache,
  // Sampling overrides and Advanced cards. Every English is the en_US.toml line
  // the criterion names, unchanged except model-profile-cache-ram, whose label drops
  // "(MiB)": the unit sits after the input (criterion 89).
  'model-profile-kv-cache': 'KV cache',
  'model-profile-k-cache-type': 'K-cache type',
  'model-profile-k-cache-type-hint':
    "Compresses the K half of the KV cache. Lower types use less VRAM but reduce quality.\nDefault: f16 (llama.cpp's own).\nFlag: --cache-type-k",
  'model-profile-v-cache-type': 'V-cache type',
  'model-profile-v-cache-type-hint':
    "Compresses the V half of the KV cache. Lower types use less VRAM but reduce quality; usually set to match K.\nA quantized V needs flash attention: with Flash attention off, the model does not load.\nDefault: f16 (llama.cpp's own).\nFlag: --cache-type-v",
  'model-profile-flash-attention': 'Flash attention',
  'model-profile-flash-attention-hint':
    "A faster attention method: faster prefill and less KV memory.\nDefault: auto (llama.cpp's own): on wherever the backend supports it.\non forces it. off disables it, which loses most of the gain of a quantized K cache and stops a quantized V cache from loading.\nFlag: --flash-attn",
  'model-profile-cache-ram': 'Cache RAM',
  'model-profile-cache-ram-hint':
    "How much RAM may hold past prompts for reuse, in MiB, so they need not be read again. Context checkpoints count against it: on a large hybrid model with long prompts and several slots, old prompts are dropped sooner than their KV size suggests.\nDefault: 8192 MiB (llama.cpp's own). 0 = off, -1 = no limit.\nFlag: --cache-ram",
  'model-profile-cache-reuse': 'Cache reuse',
  'model-profile-cache-reuse-hint':
    "Lets a new prompt reuse matching pieces of the cached one by shifting the KV cache, instead of reading them again. The value is the smallest piece worth reusing, in tokens.\nDefault: 0, off (llama.cpp's own). llama.cpp also turns it off, with a warning in the log, on a model with an mmproj and on one whose cache cannot shift.\nFlag: --cache-reuse",
  'model-profile-context-checkpoints': 'Context checkpoints',
  'model-profile-context-checkpoints-hint':
    "How many saved states (checkpoints) each slot keeps, so a prompt that changes partway need not be read again from the start. Matters on recurrent, hybrid and SWA models (Qwen3.5/3.8, qwen4exp, Mamba, Jamba...), which cannot rewind: without checkpoints they read the whole prompt again. No effect on a pure-attention model.\nEach checkpoint is a copy of that state in RAM; the oldest is dropped once the slot holds this many.\nDefault: 32 (llama.cpp's own). 0 = off.\nFlag: --ctx-checkpoints",
  'model-profile-checkpoint-step': 'Checkpoint step',
  'model-profile-checkpoint-step-hint':
    "The minimum distance between two checkpoints, in tokens. Smaller = a changed prompt resumes closer to the change, but more checkpoints use more RAM (those of a cached prompt also count against Cache RAM).\nDefault: 8192 (llama.cpp's own). 0 = no minimum.\nFlag: --checkpoint-min-step",
  'model-profile-sampling-overrides': 'Sampling overrides',
  'model-profile-temperature': 'Temperature',
  'model-profile-temperature-hint':
    "How adventurous the output is. Lower = more predictable, higher = more creative.\nDefault: 0.80 (llama.cpp's own).\nFlag: --temp",
  'model-profile-top-k-hint':
    "Considers only the K likeliest next tokens. 0 = off.\nDefault: 40 (llama.cpp's own).\nFlag: --top-k",
  'model-profile-top-p-hint':
    "Considers only the smallest set of likely tokens whose probabilities add up to P (such as 0.95).\nDefault: 0.95 (llama.cpp's own).\nFlag: --top-p",
  'model-profile-min-p-hint':
    "Drops tokens less likely than this fraction of the likeliest one.\nDefault: 0.05 (llama.cpp's own).\nFlag: --min-p",
  'model-profile-repeat-penalty': 'Repeat penalty',
  'model-profile-repeat-penalty-hint':
    "Above 1 discourages repeating recent tokens (such as 1.1).\nDefault: 1.0, off (llama.cpp's own).\nFlag: --repeat-penalty",
  'model-profile-presence-penalty': 'Presence penalty',
  'model-profile-presence-penalty-hint':
    "Positive values discourage any token already seen, pushing toward new topics.\nDefault: 0.0, off (llama.cpp's own).\nFlag: --presence-penalty",
  'model-profile-backend-sampling': 'Backend sampling',
  'model-profile-backend-sampling-hint':
    "Experimental: picks the next token on the GPU that holds the model's output, instead of copying that output to the CPU first.\nOnly some samplers can run there (greedy, dist, top-k, top-p, min-p, temp, penalties, logit-bias). Any other in the chain (typical, xtc, mirostat, top-n-sigma, dry, adaptive-p) silently sends the WHOLE chain back to the CPU.\nTurned off by a grammar or JSON schema, by a Thinking budget, and by split mode tensor. A request asking for pre-sampling probabilities uses the CPU for that request.\nDefault: off (llama.cpp's own); on is the one value that changes anything. An MTP or DFlash drafter already samples on the GPU, whatever this says.\nFlag: --backend-sampling",
  'model-profile-advanced': 'Advanced',
  'model-profile-load-on-startup': 'Load on startup',
  'model-profile-load-on-startup-hint':
    'Loads this model as soon as the server starts, instead of on its first request. Read only at startup: a model profile added while the server runs is loaded on demand.\nThe server refuses to start when more model profiles load on startup than ModelsMax allows, and with ModelsMax = 0 whenever any does, so Save refuses that too.\nKey: load-on-startup (model profile only, no command-line flag)',
  'model-profile-chat-kwargs': 'Chat kwargs',
  'model-profile-chat-kwargs-hint':
    'Extra values passed to the chat template, as JSON, such as {"enable_thinking":true}.\nAdvanced: leave blank unless the model\'s documentation lists them.\nFlag: --chat-template-kwargs',
  // Slice 2e.3 (fe/briefs/02e-model-profile-cards.md, criterion 105): the Chat /
  // reasoning and Vision cards. Every English is the en_US.toml line the
  // criterion names, unchanged.
  'model-profile-chat-reasoning': 'Chat / reasoning',
  'model-profile-chat-template': 'Chat template',
  'model-profile-chat-template-hint':
    'Uses the chat template embedded in the model to format conversations, instead of built-in guesses. Recommended on, for correct tool calls and roles.\nFlag: --jinja',
  'model-profile-chat-template-toggle': 'use embedded chat template (--jinja)',
  'model-profile-prefill-assistant': 'Prefill assistant',
  'model-profile-prefill-assistant-hint':
    "What to do when a request ends with an assistant message.\nOn (default, llama.cpp's own): treat it as the start of the reply and continue it, so a client can seed the answer's opening.\nOff: treat it as a finished turn and answer it. Turn it off if a client ends on an assistant turn and the model resumes mid-sentence instead of replying.\nFlags: --prefill-assistant, --no-prefill-assistant",
  'model-profile-prefill-assistant-toggle':
    'continue a trailing assistant message (--prefill-assistant)',
  'model-profile-reasoning': 'Reasoning',
  'model-profile-reasoning-hint':
    "Whether the model thinks before answering.\nauto (recommended): follows the model's template.\non: force thinking.\noff: disable it.\nFlag: --reasoning",
  'model-profile-reasoning-format': 'Reasoning format',
  'model-profile-reasoning-format-hint':
    'How thinking is delivered to the client.\nauto = let the template decide.\ndeepseek = thinking goes in a separate channel, apart from the answer.\nnone = left inline in the answer.\nFlag: --reasoning-format',
  'model-profile-reasoning-effort': 'Reasoning effort',
  'model-profile-reasoning-effort-hint':
    "How hard the template asks the model to think: a level handed to the chat template, neither an on/off switch nor a token limit.\nDefault: whatever the template does on its own.\nOnly some templates read it (gpt-oss and harmony do, most do not); the others ignore it silently, and llama-server's log does not say. The Thinking row in Model info is the place that tells you.\nSet it here, not in Chat kwargs below: this one setting binds both names templates use (reasoning_effort, reasoning_strength), and a hand-written one sets only one of them.\nFlag: --reasoning-effort",
  'model-profile-keep-past-thinking': 'Keep past thinking',
  'model-profile-keep-past-thinking-hint':
    "Sends the thinking of EVERY past assistant turn back to the model, not just the last one. It costs prompt tokens: the thinking is re-sent every turn.\nDefault: on (llama.cpp's own). on pins it; off drops it, the one value that changes anything.\nOnly works where the template supports it: llama-server says so in its log at startup.\nUse this, not preserve_thinking or preserve_reasoning in Chat kwargs below: templates read different names, so a hand-written one usually does nothing.\nFlags: --reasoning-preserve, --no-reasoning-preserve",
  'model-profile-output-budget-note':
    "The next three split one output budget in two. Thinking budget limits the thinking alone; Max output limits thinking and answer together, so the answer gets what the thinking leaves. Do not copy a model card's numbers as they are: they assume a certain context, and a budget larger than your context never takes effect.",
  'model-profile-thinking-budget': 'Thinking budget',
  'model-profile-thinking-budget-hint':
    "The most tokens the model may spend thinking. Pick it by how long you are willing to wait.\n-1 = no limit, 0 = end thinking immediately, N > 0 = that many tokens.\nDefault: -1 (llama.cpp's own).\nAt the limit llama.cpp closes the thinking block for the model, which still writes a normal answer. Only works where the template declares thinking tags.\nFlag: --reasoning-budget",
  'model-profile-budget-message': 'Budget message',
  'model-profile-budget-message-hint':
    "Text added just before thinking is closed at the budget, to steer the model into wrapping up instead of stopping mid-thought. Write it in the language the model answers in: another language invites mixed replies.\nBlank = no message (llama.cpp's own default).\nNo ';' or '#': the INI format cannot escape them, and the value would reload truncated.\nFlag: --reasoning-budget-message",
  'model-profile-max-output': 'Max output',
  'model-profile-max-output-hint':
    "The most tokens one request may generate, thinking and answer together, for clients that do not set their own limit.\n-1 = keep going until the context is full.\nDefault: -1 (llama.cpp's own).\nNot a hard limit: a client's own max_tokens always wins, higher or lower.\nFlag: --n-predict",
  'option-auto': 'auto',
  'option-minimal': 'minimal',
  'option-low': 'low',
  'option-medium': 'medium',
  'option-high': 'high',
  'option-extra-high': 'xhigh',
  'option-maximum': 'max',
  'model-profile-vision': 'Vision',
  'model-profile-vision-on-gpu': 'Vision on GPU',
  'model-profile-vision-on-gpu-hint':
    "Runs the image encoder (mmproj) on the GPU. On (default, llama.cpp's own), it holds VRAM as long as the model is loaded but only works on image requests. Off: slower image encoding, no VRAM.\nWhich GPU: see 'Vision GPU' below.\nFlags: --mmproj-offload, --no-mmproj-offload",
  'model-profile-vision-offload-toggle': 'offload the image encoder to GPU',
  'model-profile-vision-gpu': 'Vision GPU',
  'model-profile-vision-gpu-hint':
    "The GPU the image encoder runs on.\nDefault: the first GPU in this model profile's device list; with no device checked, the first GPU llama.cpp finds, which on a mixed machine is often not the one running the model. It holds VRAM there and stays idle until an image arrives.\nName a device to override both.\nFlag: --mmproj-device",
  // 2h.7's 176, 182: the note shown while the model profile has no MMProj.
  'model-profile-vision-off':
    'Used only with an MMProj file, and this model profile has none (Assets).',
  'model-profile-image-tokens-note':
    'The two limits below set how many tokens ONE image takes on dynamic-resolution vision models. They apply only with an MMProj picked above. Leave them at default to read the limit from the model.',
  'model-profile-min-image-tokens': 'Min image tokens',
  'model-profile-min-image-tokens-hint':
    'The fewest tokens each image is encoded to, on vision models with dynamic resolution.\nDefault: read from the model.\nFlag: --image-min-tokens',
  'model-profile-max-image-tokens': 'Max image tokens',
  'model-profile-max-image-tokens-hint':
    'The most tokens each image is encoded to, on vision models with dynamic resolution. Lower it to limit the prefill cost and VRAM of large images.\nDefault: read from the model.\nFlag: --image-max-tokens',
  'model-profile-device-follows-model': '(default: follows device)',
  // Slice 2e.3b (criterion 124): the line naming how the file spells a flag.
  'model-profile-written-as': 'Written as {0}',
  'model-profile-written-empty': 'Written with no value',
  // 2h.7's 177, 178, 182: the two note rows of the Hardware card.
  'model-profile-draft-file-off':
    'Draft device and Draft GPU layers apply only to a Draft model file, and this model profile has none (Assets).',
  'model-profile-moe-later':
    "MoE cache stays as the file has it: its floor needs the model's tensors, which this page does not read yet. Change it in pai-config on the server machine.",
  'model-profile-hardware-config': 'Hardware config',
  'model-profile-gpu-layers': 'GPU layers',
  'model-profile-gpu-layers-hint':
    "How many of the model's layers run on the GPU. The rest stay on the CPU: less VRAM, slower.\nSlider maximum = all layers, once the model file is read.\nDefault: all layers on the GPU. A model too big for it fails to load, unless Fit to VRAM is on.\nFlag: --n-gpu-layers",
  'model-profile-moe-cpu-layers': 'MoE CPU layers',
  'model-profile-moe-cpu-layers-hint':
    "Frees VRAM by keeping the experts of this many layers on the CPU; generation gets slower. Only for MoE models: on a dense model use 'Dense FFN CPU layers' below.\nSlider maximum = the model's layer count, once the model file is read.\nDefault: 0, every expert stays on the GPU with its layer.\nFlag: --n-cpu-moe",
  'model-profile-moe-cache': 'MoE cache',
  'model-profile-moe-cache-hint':
    "VRAM set aside for recently used experts kept in RAM, so a token that needs them reads them from the card instead of over PCIe. Experimental.\nIt only acts on layers whose experts are ALL in RAM (MoE CPU layers above, or an exps=CPU Tensor placement rule); with none, llama.cpp runs without a cache.\nWith several GPUs the budget is split like the layers: by Tensor split, or by free VRAM when that is blank.\nThe model FAILS TO LOAD under Split mode tensor, and below the experts of one token.\nThe draft model never uses it, and the synthetic benchmark cannot measure it.\nDefault: 0, no cache (llama.cpp's own).\nFlag: --moe-cache-mib",
  'model-profile-dense-ffn-cpu-layers': 'Dense FFN CPU layers',
  'model-profile-dense-ffn-cpu-layers-hint':
    "Frees VRAM by keeping the feed-forward weights (most of a layer's size) of the first N layers on the CPU; attention stays on the GPU. For dense models: on a MoE model it reaches only the dense layers such a model may have, never the experts.\nDefault: every feed-forward weight stays on the GPU with its layer.\nFlag: --n-cpu-ffn",
  'model-profile-op-offload': 'Op offload',
  'model-profile-op-offload-hint':
    "Whether work on weights kept in RAM is still sent to the GPU. Worth trying when experts or other tensors are in RAM and the card sits on a narrow PCIe link: off then prefills faster, because those weights are no longer copied to the card on every prefill batch (the Qwen3.8-Flash-Next guide has the figures). Decode does not change.\nDefault: on (llama.cpp's own); off is the one value that changes anything. No effect when every weight is in VRAM.\nFlags: --op-offload, --no-op-offload",
  'model-profile-repack': 'Repack',
  'model-profile-repack-hint':
    "When loading, rearranges weights kept in RAM into the layout the CPU's fast kernels use, for the types it has them for (x86 AVX2: Q4_0, Q4_K, IQ4_NL, MXFP4; Q2_K with AVX-512).\nOnly matters without a GPU, or with No host buffer on: with a GPU, weights in RAM go to its host buffer and this does nothing.\nDefault: on (llama.cpp's own); off is the one value that changes anything. No effect when no weight is in RAM.\nFlags: --repack, --no-repack",
  'model-profile-no-host-buffer': 'No host buffer',
  'model-profile-no-host-buffer-hint':
    "Keeps weights left in RAM in ordinary memory instead of the GPU's pinned host buffer (RAM locked in place, which the GPU reads directly).\nOff (default, llama.cpp's own): pinned.\nOn: ordinary, pageable RAM, and the CPU repacks the types it can (see Repack). The copies Op offload makes then come from pageable memory, and repacked experts leave the MoE cache.\nNo effect when no weight is in RAM.\nFlag: --no-host",
  'model-profile-lazy-tensors': 'Lazy tensors',
  'model-profile-lazy-tensors-hint':
    "Saves RAM on models with very large tables: reads them from disk as needed, a row per token, instead of keeping them in memory. Only a few architectures have such tables (such as gemma4 and qwen4exp); no effect on others. Keep the GGUF on an SSD.\nDefault: auto (llama.cpp's own): lazy only for a table over 4 GiB, and off if one of the model's devices cannot memory-map (an integrated GPU).\non = lazy whatever the size and the devices; off = always in RAM (no disk reads, but all of the RAM).\nWorks whatever Load mode says.\nFlag: --lazy-mode",
  'model-profile-load-mode': 'Load mode',
  'model-profile-load-mode-hint':
    "How this model's weights get into memory. Pick one: the modes do not combine.\nDefault: auto (llama.cpp's own).\nauto: memory-maps the file (reads it on demand) unless a device can't; fastest to load.\nnone: reads the weights into RAM up front: slower, uses the model's size in RAM, but nothing can be dropped back to the file.\nmmap: always memory-maps.\nmlock: locks the weights in physical RAM WITHOUT memory-mapping: steady latency, needs room for the whole model.\nmmap+mlock: memory-maps and locks what it maps.\ndio: Direct I/O where the platform offers it.\nLazy tensors stay mapped whatever this says.\nFlag: --load-mode",
  'model-profile-fit-to-vram': 'Fit to VRAM',
  'model-profile-fit-to-vram-hint':
    'Lets llama.cpp pick the context size and GPU layers that fit your GPU memory, leaving about 1 GiB free on each GPU.\nOn by default, as in llama.cpp. For a full fit, leave Context size and GPU layers at default.\nWhen on, also leave tensor-split, Tensor placement, MoE CPU layers and Dense FFN CPU layers empty: with any of them llama.cpp fits only the context (the log warns), which can over-fill one card.\nFlag: -fit',
  'model-profile-fit-allowed':
    'auto-fit context and layers to device memory (-fit on)',
  'model-profile-fit-not-allowed':
    'needs Context size and GPU layers at default',
  'model-profile-fit-invalid':
    'llama-server refuses this value, so it cannot run this model profile. Use on or off.',
  // The three lose the trailing period, to read as one family with
  // model-profile-fit-allowed and model-profile-fit-not-allowed (2h.8 review).
  'model-profile-fit-layers-only':
    'auto-fit the GPU layers only: Context size is set (-fit on)',
  'model-profile-fit-context-only':
    'auto-fit the context only: GPU layers is set, so llama.cpp keeps your layers (-fit on)',
  'model-profile-fit-nothing':
    'nothing to fit: Context size and GPU layers are both set (-fit on)',
  'model-profile-fit-min-context': 'Fit min context',
  'model-profile-fit-min-context-hint':
    "The smallest context Fit to VRAM may cut down to. Only used with Fit to VRAM on.\nDefault: 4096 (llama.cpp's own).\nFlag: --fit-ctx",
  'model-profile-fit-free-vram': 'Fit free VRAM',
  'model-profile-fit-free-vram-hint':
    "How much VRAM Fit to VRAM leaves free, in MiB: one number for every device, or a comma-separated list in device order (such as 2048,1024). Only used with Fit to VRAM on.\nBlank = 1024 per device (llama.cpp's own).\nFlag: --fit-target",
  'model-profile-draft-device': 'Draft device',
  'model-profile-draft-device-hint':
    "The GPU for a separate draft model file. Pin it to ONE device: MTP heads crash on an automatic multi-GPU split.\nA full-vocabulary DFlash or DSpark drafter under a layer split must sit on the GPU with the last layers, or it aborts at load; picking the file pins it there for you.\nDisabled with built-in MTP heads, which share the main model's GPU.\nBlank = automatic, same as the model.\nFlag: --device-draft",
  'model-profile-draft-gpu-layers': 'Draft GPU layers',
  'model-profile-draft-gpu-layers-hint':
    'How many layers of a separate draft model file run on the GPU.\nFor an MTP head file, which has no layers, 99 puts the draft on the GPU and 0 on the CPU.\nPin a device above: MTP heads crash on an automatic multi-GPU split.\nDisabled with built-in MTP heads.\nDefault: all draft layers on the GPU.\nFlag: --n-gpu-layers-draft',
  'model-profile-device-same-as-model': '(auto / same as model)',
  // Brief 14 (fe/briefs/14-gpu-distribution.md), section 2: the GPU distribution
  // card. GD1's keys, from model-profile-gpu-distribution to
  // model-profile-gpu-moved and api-device-bad-name, then GD2's (the weights).
  // Every English is the brief's, the web text of the column that says "web".
  'model-profile-gpu-distribution': 'GPU distribution',
  'model-profile-gpu-distribution-intro':
    "Which GPUs run THIS model and how much each holds, saved to models.ini. Check nothing to let llama.cpp choose. With two GPUs of different sizes, check both and give the bigger one more; left on Auto, the split follows free VRAM at load, not an even cut. Under split mode layer, once the model file is read, each GPU takes a number of blocks: the model's blocks and its output layer, shared out top to bottom. Otherwise the weights are ratios: 3 and 1 give 75% and 25%. The checked GPUs, top to bottom, are the split order: Move up and Move down change it; the first one is llama.cpp's main GPU.\nKeys: split-mode, device, tensor-split.",
  'model-profile-split-mode': 'GPU split mode',
  'model-profile-split-mode-hint':
    'How THIS model is divided between GPUs. Does nothing with one GPU checked. Same on CUDA and ROCm.\nlayer (default): each GPU takes a consecutive run of blocks, in the proportion of its weight. Fastest prefill; copes with a slow link between GPUs.\nnone: one GPU, the first checked row; other checked rows go unused.\ntensor (experimental): the GPUs act as one device, and all weights and the KV cache are divided by the ratio (blank = even). Fastest decode, needs a fast link. Flash attention must not be off, Fit to VRAM is skipped (set Context size yourself), some architectures refuse to load, and llama.cpp tunes it for CUDA with no guarantees on other backends.\nrow (deprecated, use tensor): weights are split by rows by the ratio (CUDA/ROCm; Vulkan falls back to layer). The KV cache follows the layer cut.\nFlag: --split-mode',
  'model-profile-split-layer': 'layer: split by blocks (default)',
  'model-profile-split-none': 'none: single GPU',
  'model-profile-split-tensor':
    'tensor: tensor parallel across GPUs (experimental)',
  'model-profile-split-row': 'row: split matrices by rows (deprecated)',
  'model-profile-gpus': 'GPUs',
  'model-profile-gpu-detecting': 'Detecting GPUs…',
  'model-profile-gpu-failed':
    "The host's GPU list could not be read. The GPUs this profile names are shown; to add another, open the page again.",
  'model-profile-gpu-no-probe':
    "No GPU list: the engine's device probe gave no answer on this host.",
  'model-profile-gpu-none-found': 'No GPU found on this host.',
  'model-profile-gpu-probed-at': 'Free memory as of {0}.',
  'model-profile-gpu-memory': '{0} GiB, {1} GiB free',
  'model-profile-gpu-not-detected': '(not detected)',
  'model-profile-gpu-move-up': 'Move up',
  'model-profile-gpu-move-down': 'Move down',
  'model-profile-gpu-moved': '{0}: position {1} of {2}',
  'api-device-bad-name':
    '{0}: `{1}` is not a device llama.cpp accepts: no spaces, none of ; # =, not CPU, and none only alone',
  // GD2: the weights (tensor-split).
  'model-profile-gpu-weight': 'Weight',
  'model-profile-gpu-share': 'Share: {0}',
  'model-profile-gpu-share-auto': 'auto',
  'model-profile-gpu-share-even': 'even',
  'model-profile-gpu-share-unused': 'unused',
  'model-profile-gpu-auto': 'Auto',
  'model-profile-gpu-even': 'Even',
  'model-profile-gpu-weights-incomplete':
    'Give every checked GPU a weight of 0 or more, or clear them all for Auto.',
  'model-profile-gpu-split-without-device':
    "This profile's tensor-split {0} applies to every GPU llama.cpp finds, in its order. Checking GPUs here replaces it.",
  'api-tensor-split-format':
    'tensor-split = `{0}` is not a list of up to 15 numbers of 0 or more, separated by , or /',
  // Brief 21 (fe/briefs/21-tensor-placement.md), section 2, sub-slice TP1: the
  // Tensor placement card. Every English is the brief's.
  'model-profile-tensor-placement': 'Tensor placement',
  'model-profile-tensor-placement-intro':
    'Exceptions to the GPU distribution above: tensors that go on a device of their own, whatever their layer does. On a MoE model, sending the experts of some layers to the CPU frees the most VRAM, as MoE CPU layers does for the first ones. llama.cpp also keeps the embedding table in RAM even when it reports every layer on the GPU: put it on a GPU to reclaim that memory, if the card has room. The first rule that matches a tensor wins.\nKey: override-tensor.',
  'model-profile-ot-rules': 'Rules',
  'model-profile-ot-none': 'No rules: every tensor goes where its layer goes.',
  'model-profile-ot-format': String.raw`Layers: empty for every layer, or numbers and ranges such as 10-12, 15. Pattern: a regular expression searched in each tensor's name, such as blk\.1[0-9]\..*_exps.`,
  'model-profile-ot-rule': 'Rule {0}',
  'model-profile-ot-tensor': 'Tensor',
  'model-profile-ot-layers': 'Layers',
  'model-profile-ot-pattern': 'Pattern',
  'model-profile-ot-goes-on': 'Goes on',
  'model-profile-ot-cpu': 'CPU: system memory',
  'model-profile-ot-no-device': '(no device)',
  'model-profile-ot-add': 'Add rule',
  'model-profile-ot-remove': 'Remove',
  'model-profile-ot-kind-embd': 'Embedding table (token_embd)',
  'model-profile-ot-kind-ple': 'Per-layer embeddings (per_layer_token_embd)',
  'model-profile-ot-kind-output': 'Output head (output)',
  'model-profile-ot-kind-layer': 'Whole layer (blk.N.*)',
  'model-profile-ot-kind-attn': 'Attention (attn_*)',
  'model-profile-ot-kind-ssm': 'Linear attention / SSM (ssm_*)',
  'model-profile-ot-kind-ffn': 'Dense FFN (ffn_up/down/gate)',
  'model-profile-ot-kind-ffn_up': 'Dense FFN up (ffn_up)',
  'model-profile-ot-kind-ffn_gate': 'Dense FFN gate (ffn_gate)',
  'model-profile-ot-kind-ffn_down': 'Dense FFN down (ffn_down)',
  'model-profile-ot-kind-exps': 'MoE experts (ffn_*_exps)',
  'model-profile-ot-kind-exps_up': 'MoE experts up (ffn_up_exps)',
  'model-profile-ot-kind-exps_gate': 'MoE experts gate (ffn_gate_exps)',
  'model-profile-ot-kind-exps_down': 'MoE experts down (ffn_down_exps)',
  'model-profile-ot-kind-shexp': 'MoE shared expert (ffn_*_shexp)',
  'model-profile-ot-kind-router': 'MoE router (ffn_gate_inp)',
  'model-profile-ot-kind-custom': 'Custom regex…',
  // Brief 19 (fe/briefs/19-layer-sliders.md), section 2, sub-slice LS1: the
  // model facts read and the per-GPU block controls. LS1 adds the keys from
  // model-profile-model-facts-reading to model-profile-gpu-output-layer and
  // changes the card's intro; the rest of the table is LS2's.
  'model-profile-model-facts-reading': 'Reading the model file…',
  'model-profile-model-facts': 'Model file: {0} blocks.',
  'model-profile-model-facts-moe':
    'Model file: {0} blocks, with experts: {1} per layer, {2} used per token.',
  'model-profile-model-facts-moe-plain':
    'Model file: {0} blocks, with experts.',
  'model-profile-model-facts-none':
    'No layer counts: {0}. The GPU weights stay ratios, and the layer fields have no slider.',
  'model-profile-model-facts-failed':
    "The model file's layer counts could not be read. The GPU weights stay ratios, and the layer fields have no slider; open the page again to retry.",
  'api-model-info-fetched':
    'its model is fetched at startup (`{0}`), so its layer facts are unknown until it is downloaded',
  'api-model-info-missing': 'the model file `{0}` is not there',
  'api-model-info-unreadable': '`{0}` is not a readable model file',
  'model-profile-gpu-blocks': 'Blocks',
  'model-profile-gpu-blocks-budget':
    'The checked GPUs share {0} places: {1} blocks and the output layer.',
  'model-profile-gpu-blocks-auto':
    'Auto: llama.cpp shares the {0} places by free memory at load. Choose Even to set the blocks yourself.',
  'model-profile-gpu-blocks-malformed':
    'tensor-split {0} is not a list of numbers of 0 or more. Choose Even to set the blocks yourself, or Auto.',
  'model-profile-gpu-blocks-value': '{0} of {1}: {2}',
  'model-profile-gpu-blocks-nothing': 'nothing',
  'model-profile-gpu-block-one': 'block {0}',
  'model-profile-gpu-blocks-range': 'blocks {0}-{1}',
  'model-profile-gpu-block-plus-output': 'block {0} + output',
  'model-profile-gpu-blocks-plus-output': 'blocks {0}-{1} + output',
  'model-profile-gpu-output-layer': 'output layer',
  // LS2a (19 G.13, 19 G.14): the layer sliders' value text, and the two reason
  // paragraphs of MoE CPU layers. The three changed rows of section 2 are
  // amended above, at their own keys.
  'model-profile-layers-value': '{0} of {1}',
  'model-profile-moe-reading':
    'Reading the model file to see whether it has experts…',
  'model-profile-moe-dense':
    'This model has no experts (a dense model), so MoE CPU layers does nothing for it. Use Dense FFN CPU layers below.',
  // LS2b (19 G.16 to G.19, fe/briefs/19-layer-sliders.md section 2): the
  // device lists of the Draft device and Vision GPU selects, and the device
  // bound refusal. The English is the brief's, verbatim; the refusal line has
  // no closing full stop (02h 150).
  'model-profile-device-option': '{0}: {1}',
  'model-profile-device-option-missing': '{0} (not detected)',
  'model-profile-device-none': 'none: not on a GPU',
  'api-model-profile-device-outside':
    "{2} = {3} names a GPU outside the Devices of instance {1}'s server profile, so model profile {0} may not use it. Pick a GPU of those Devices, or widen the server profile's Devices first",
  // Slice 2e.5 (fe/briefs/02e-model-profile-cards.md, criterion 116): the Speculative
  // decoding card. Every English is the en_US.toml line the criterion names
  // (367 to 397), unchanged.
  'model-profile-speculative-decoding': 'Speculative decoding',
  'model-profile-draft-model-group':
    'Draft model (draft-mtp, draft-dflash, draft-dspark, draft-simple)',
  'model-profile-draft-n-max': 'Draft n-max',
  'model-profile-draft-n-max-hint':
    "The most tokens a draft proposes per step. DFlash limits this to the size it was trained for, minus one (the Draft file row shows it).\nDefault: 3 (llama.cpp's own).\nFlag: --spec-draft-n-max",
  'model-profile-draft-p-min': 'Draft p-min',
  'model-profile-draft-p-min-hint':
    "How sure the drafter must be, 0 to 1. Drafting stops at the first token below this, so the model checks a shorter batch instead of tokens it would reject.\nDefault: 0, never stops early (llama.cpp's own).\nFlag: --spec-draft-p-min",
  'model-profile-draft-n-min': 'Draft n-min',
  'model-profile-draft-n-min-hint':
    "The shortest draft worth checking. A shorter one, usually cut by Draft p-min, is thrown away, and that step generates one token normally. Raise it when short drafts cost more to check than they save.\nDefault: 0, keeps any length (llama.cpp's own).\nFlag: --spec-draft-n-min",
  'model-profile-draft-n-ubatch': 'Draft n-ubatch',
  'model-profile-draft-n-ubatch-hint':
    "How many tokens the draft computes at once. Smaller spares VRAM, at the cost of more passes when the drafter catches up on a long prompt. Applies to built-in MTP heads too.\nDefault: 0, the model's U-Batch size (llama.cpp's own).\nFlag: --spec-draft-n-ubatch",
  'model-profile-draft-cache-note':
    "The draft keeps its OWN KV cache, which does not follow the model's: it stays at f16 unless you set it here. Quantize it to reclaim VRAM. Unlike the placement settings above, these two also apply to MTP heads built into the main model.",
  'model-profile-draft-k-cache-type': 'Draft K-cache type',
  'model-profile-draft-k-cache-type-hint':
    "Compresses the K half of the draft's KV cache. A draft context is small, so this matters most with a separate draft model and a large context.\nDefault: f16, NOT the model's K-cache type.\nFlag: --spec-draft-type-k",
  'model-profile-draft-v-cache-type': 'Draft V-cache type',
  'model-profile-draft-v-cache-type-hint':
    "Compresses the V half of the draft's KV cache. Usually matched to Draft K-cache type. A quantized V needs flash attention.\nDefault: f16, NOT the model's V-cache type.\nFlag: --spec-draft-type-v",
  'model-profile-match-length': 'Match length',
  'model-profile-match-length-hint':
    "How many recent tokens must match before a draft is proposed. Below 16, llama.cpp warns that drafts may be poor.\nDefault: 24 (llama.cpp's own).\nFlag: --spec-ngram-mod-n-match",
  'model-profile-min-draft': 'Min draft',
  'model-profile-min-draft-hint':
    "The fewest tokens a draft must have to be proposed, 0 to 1024.\nDefault: 48 (llama.cpp's own).\nFlag: --spec-ngram-mod-n-min",
  'model-profile-max-draft': 'Max draft',
  'model-profile-max-draft-hint':
    "The most tokens drafted per step, 0 to 1024.\nDefault: 64 (llama.cpp's own).\nFlag: --spec-ngram-mod-n-max",
  'model-profile-lookup-n-gram': 'Lookup n-gram',
  'model-profile-ngram-simple-lookup-hint':
    "How many recent tokens are looked up in the context (the n-gram), 1 to 1024.\nDefault: 12 (llama.cpp's own).\nFlag: --spec-ngram-simple-size-n",
  'model-profile-draft-m-gram': 'Draft m-gram',
  'model-profile-ngram-simple-draft-hint':
    "How many tokens are drafted after a match (the m-gram), 1 to 1024.\nDefault: 48 (llama.cpp's own).\nFlag: --spec-ngram-simple-size-m",
  'model-profile-ngram-map-lookup-hint':
    "How many recent tokens are looked up in the context (the n-gram), 1 to 1024.\nDefault: 12 (llama.cpp's own).\nFlag: --spec-ngram-map-k-size-n",
  'model-profile-ngram-map-draft-hint':
    "How many tokens are drafted after a match (the m-gram), 1 to 1024.\nDefault: 48 (llama.cpp's own).\nFlag: --spec-ngram-map-k-size-m",
  'model-profile-ngram-map-values-lookup-hint':
    "How many recent tokens are looked up in the context (the n-gram), 1 to 1024.\nDefault: 12 (llama.cpp's own).\nFlag: --spec-ngram-map-k4v-size-n",
  'model-profile-ngram-map-values-draft-hint':
    "How many tokens are drafted after a match (the m-gram), 1 to 1024.\nDefault: 48 (llama.cpp's own).\nFlag: --spec-ngram-map-k4v-size-m",
  'model-profile-min-hits': 'Min hits',
  'model-profile-min-hits-hint':
    "How many times an n-gram must have been seen before what follows it is proposed. Higher drafts less often but more reliably.\nDefault: 1 (llama.cpp's own).\nFlag: --spec-ngram-map-k4v-min-hits",
  // Sub-slice 2h.9 (fe/briefs/02h-model-profile-save.md, criterion 193): the two
  // reason lines of the Speculative decoding card's groups. 2h.10's 198
  // rewords the draft one: the Assets row it points to is "Draft model".
  'model-profile-draft-group-off':
    'Used only with a draft: a Draft model or a Drafter, and this model profile has neither (Assets).',
  'model-profile-ngram-off':
    'Used only when N-gram drafters (Assets) includes {0}.',
  // H11a.2 (fe/briefs/08-hearth-h10-h11.md, section 2): the line under the
  // model profile's h1, the engine and the model file it points at.
  'model-profile-summary': 'Engine {0}, model {1}',
  'model-profile-summary-no-model': 'Engine {0}, no model file',
  // Slice 13 (fe/briefs/13-node-about.md, section 2): the host in the nav, the
  // About page, and the boot state that comes with the instances list.
  'nav-about': 'About',
  'about-title': 'About',
  'about-loading': 'Loading the versions…',
  'about-failed': 'The versions could not be read.',
  'about-version': 'Version',
  'about-version-unknown': 'Unknown',
  'about-backends': 'GPU backends',
  'about-backends-none': 'None, CPU only',
  'about-no-engine':
    'No installed engine was found on this host. An engine package from before this version lists none until it is installed again.',
  'backend-cuda': 'CUDA',
  'backend-rocm': 'ROCm/HIP',
  'backend-vulkan': 'Vulkan',
  // Slice 18 AN5a (fe/briefs/18-about-node-info.md, section 2): the Node info
  // page, its nav entry, and the joined OS, driver and Vulkan rows.
  'nav-node': 'Node info',
  'node-title': 'Node info',
  'node-loading': "Loading the node's information…",
  'node-failed': "The node's information could not be read.",
  'node-host': 'Host name',
  'system-os': 'Operating system',
  'system-kernel': 'Kernel',
  'node-engines': 'Engines',
  'node-gpus': 'GPUs',
  'node-gpus-none': 'No GPU was found on this node.',
  'node-info-unavailable':
    'The manager did not answer, so the operating system, the kernel and the GPUs are not shown. Reload the page to try again.',
  'node-info-failed':
    'The manager could not read the operating system, the kernel and the GPUs.',
  'node-gpu-title': 'GPU {0}: {1}',
  'node-gpu-title-unnamed': 'GPU {0}',
  'node-gpu-vendor': 'Vendor',
  'node-gpu-pci': 'PCI address',
  'node-gpu-driver': 'Driver',
  'node-driver-in-kernel':
    "(part of the kernel, so its version is the kernel's)",
  'node-driver-none': 'No driver in use',
  'node-gpu-vulkan': 'Vulkan driver',
  'node-vulkan-unreported': 'Not reported by the device',
  'node-vulkan-not-seen': 'Not seen by Vulkan',
  'node-vulkan-none': 'No Vulkan driver installed',
  'node-vulkan-failed': 'Could not be read',
  'vendor-amd': 'AMD',
  'vendor-nvidia': 'NVIDIA',
  'vendor-intel': 'Intel',
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

/** The reply's `user`, no role: the words the banner must stop showing (criterion 4). */
export const ROLE_WORDS = ['Administrator', 'User']

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
  // One Response per reader: a body is good once, and the same held read is
  // answered to every reader of it (the shell reads the host and so does the
  // page on screen, 18 AN.15). The clones are taken as the reply is handed
  // over, before anybody reads the body.
  const twins: Response[] = []
  let readers = 0
  let settle: (r: Response) => void = () => {}
  const replied = new Promise<Response>((r) => {
    settle = r
  })
  return {
    signals,
    answer(init?: RequestInit) {
      const signal = init?.signal ?? undefined
      signals.push(signal)
      readers += 1
      return new Promise<Response>((resolve, reject) => {
        if (signal?.aborted) {
          reject(signal.reason)
          return
        }
        signal?.addEventListener('abort', () => reject(signal.reason), {
          once: true,
        })
        void replied.then((r) => resolve(twins.pop() ?? r))
      })
    },
    resolve: (r) => {
      for (let i = 1; i < readers; i++) twins.push(r.clone())
      settle(r)
    },
  }
}

/**
 * One instance as /api/instances lists it. Brief 12 (slice T1): `port` is the
 * port it listens on (0 when the manager has none), `tags` the union of the
 * server profile's tags and its own, `instanceTags` the ones its page edits.
 * Brief 13 (13 N.1): `boot` is that instance's start at boot, `null` when the
 * manager's read of that one failed; the tile shows a row only for a boolean.
 * Brief 20 (20 N.2): `serverProfile` is the name of the server profile the
 * record carries, `null` when it names none; the nav's link shows it, the tile
 * does not. The default fixtures leave it out on purpose: every test that finds
 * an instance link by the name `default` (1a.4) stays green, and a test that
 * wants the profile writes it.
 * A real answer can leave any of them out or spell it badly: the page reads
 * them forgivingly.
 */
export type Instance = {
  id: string
  engine: string
  state: string
  port?: number
  tags?: string[]
  instanceTags?: string[]
  boot?: boolean | null | string | number
  serverProfile?: string | null | number
}

export const INSTANCES: Instance[] = [
  { id: 'default', engine: 'llama.cpp', state: 'active' },
  { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
]

/**
 * One GPU backend package as GET /api/node lists it (fe/briefs/13-node-about.md
 * section 0): the package suffix, and the package's version, `null` when its
 * version file is empty.
 */
export type NodeBackend = { name: string; version: string | null }

/** One installed engine: its id, its package version, its backends. */
export type NodeEngine = {
  engine: string
  version: string | null
  backends?: NodeBackend[]
}

/** What GET /api/node answers: the host, the framework's version, the engines. */
export type NodeInfo = {
  name: string
  api: string
  version: string | null
  engines: NodeEngine[]
}

/** The operating system as brief 18 section 0 sends it: three texts, each a
 * string or null. */
export type NodeOs = {
  name: string | null
  version: string | null
  kernel: string | null
}

/** A GPU row's Vulkan driver: its driver and its info, each a string or null. */
export type NodeGpuVulkan = { driver: string | null; info: string | null }

/** One GPU row of brief 18 section 0, in the bus order of the reply. */
export type NodeGpu = {
  pci: string | null
  vendor: string | null
  name: string | null
  driver: string | null
  version: string | null
  vulkan: NodeGpuVulkan | null
}

/** What brief 18 (AN4, not built yet) adds to today's reply: the operating
 * system, the Vulkan answer, the GPUs, and the `nodeInfo` key the bff sends
 * when the manager did not answer. */
export type NodeExtra = {
  os?: NodeOs | null
  vulkan?: 'ok' | 'none' | 'failed' | null
  gpus?: NodeGpu[] | null
  nodeInfo?: { key: string; args: unknown[] } | null
}

/** The three GPUs of brief 18 section 7, the same rows `fe/ux/stub.py` sends. */
export const GPUS: NodeGpu[] = [
  {
    pci: '0000:03:00.0',
    vendor: '1002',
    name: 'AMD Radeon AI PRO R9700',
    driver: 'amdgpu',
    version: '7.1.9.31600000',
    vulkan: { driver: 'radv', info: 'Mesa 25.2.0' },
  },
  {
    pci: '0000:0a:00.0',
    vendor: '10de',
    name: 'NVIDIA GeForce RTX 5090',
    driver: 'nvidia',
    version: '580.95.05',
    vulkan: { driver: 'NVIDIA', info: '580.95.05' },
  },
  {
    pci: '0000:0f:00.0',
    vendor: '8086',
    name: 'Intel Arc B580',
    driver: 'xe',
    version: null,
    vulkan: null,
  },
]

/** The operating system of brief 18 section 0. */
export const OS: NodeOs = {
  name: 'Ubuntu',
  version: '26.04',
  kernel: '7.0.0-43-generic',
}

/** The reply of fe/briefs/13-node-about.md section 0, with brief 18's `os`,
 * `vulkan` and three `gpus`: the whole host, as the Node info page reads it. */
export const NODE: NodeInfo & NodeExtra = {
  name: 'gpu-box',
  api: '/api',
  version: '2.99.1',
  engines: [
    {
      engine: 'llama.cpp',
      version: '1:1.0.0+pai2',
      backends: [
        { name: 'cuda', version: '13.5.0+nccl.2.32.3' },
        { name: 'rocm', version: '10.1.0' },
        { name: 'vulkan', version: '1.4.363.1' },
      ],
    },
  ],
  os: OS,
  vulkan: 'ok',
  gpus: GPUS,
}

/** What GET /api/version answers (brief 18 section 0, as amended after its
 * section 9): the framework's version, the host name the bff reads for the
 * shell (AN.15), and the operating system. */
export type VersionBody = {
  version: string | null
  name?: string | null
  os?: NodeOs | null
}

/** The reply of brief 18 section 0, without `name`: the fake's default answers
 * no host name, so every test that finds the nav's fallback label "Instances"
 * (13 N.4) stays green. A test that wants the host writes
 * `{ ...VERSION, name: NODE.name }`. */
export const VERSION: VersionBody = { version: '2.99.1', os: OS }

/** The same reply carrying the host: what the shell reads (AN.15). Its name is
 *  a plain string, so a test can look for it by name. */
export const VERSION_HOST: VersionBody & { name: string } = {
  ...VERSION,
  name: NODE.name,
}

/**
 * Every way the shell's one host read (GET /api/version) can go wrong. None of
 * them ends the session and none of them moves a label off its fallback:
 * 13 N.3 and 18 AN.15 pin that, 20 N.4 pins the same for the host entry of the
 * Nodes group. Kept here so both suites walk the same list.
 */
export const HOST_FAILURES: Array<
  [string, () => Response | Promise<Response>]
> = [
  [
    '401 session-required',
    () => reply(401, { key: 'session-required', args: [] }),
  ],
  [
    '403 session-not-administrator',
    () => reply(403, { key: 'session-not-administrator', args: [] }),
  ],
  ['500', () => reply(500, 'a manager detail')],
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
  ],
  ['a network failure', () => Promise.reject(new TypeError('failed to fetch'))],
  ['a body that is not json', () => reply(200, 'not json')],
  ['a body that is not an object', () => reply(200, [])],
  ['the reply of 13 N.10, with no name', () => reply(200, VERSION)],
  ['a name that is not a string', () => reply(200, { ...VERSION, name: 7 })],
  ['a name that is empty', () => reply(200, { ...VERSION, name: '' })],
]

/**
 * One GPU as GET /api/node/devices lists it (fe/briefs/14-gpu-distribution.md
 * section 0): the engine's id, its product name, the backend that probed it and
 * its memory in MiB at the probe. A real answer can leave any of them out or
 * spell it badly: the page reads them forgivingly (GD1.7).
 */
export type Device = {
  id?: string | null
  name?: string | number | null
  backend?: string
  totalMib?: number | string | null
  freeMib?: number | string | null
}

/** What GET /api/node/devices answers: the probe's time (RFC 3339 UTC, `null`
 * when no probe answered), and the devices in the engine's probe order. */
export type DeviceProbe = { probedAt?: string | null; devices?: Device[] }

/** The brief's reply of section 0: two discrete cards and an integrated GPU, in
 * probe order, the CPU left out. */
export const DEVICES: DeviceProbe = {
  probedAt: '2026-11-20T09:14:03Z',
  devices: [
    {
      id: 'CUDA0',
      name: 'NVIDIA GeForce RTX 5080',
      backend: 'CUDA',
      totalMib: 16304,
      freeMib: 15200,
    },
    {
      id: 'ROCm0',
      name: 'AMD Radeon AI PRO R9700',
      backend: 'ROCm',
      totalMib: 32624,
      freeMib: 32420,
    },
    {
      id: 'Vulkan0',
      name: 'Intel Arc Graphics',
      backend: 'Vulkan',
      totalMib: 8129,
      freeMib: 7900,
    },
  ],
}

/** The `model` path of `modelProfileDoc()`'s default section. */
export const MODEL_FILE =
  '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/**
 * The `<arch>.block_count` and expert counts of one model file, as
 * GET /api/profiles/model/{id}/model-info answers them (fe/briefs/19-layer-sliders.md
 * section 0): `blockCount` every block of the GGUF (the nextn/MTP ones included),
 * `expertCount` and `expertUsed` 0 when the header has none. A real answer can
 * leave a key out or spell it badly, as the device probe's can (GD1.7).
 */
export type ModelFile = {
  blockCount?: number | string | null
  moe?: boolean | null
  expertCount?: number | null
  expertUsed?: number | null
}

/**
 * One GET /api/profiles/model/{id}/model-info answer (brief 19 section 0): the
 * `model` the manager read (null when the section names none), the file's facts
 * (null exactly when `reason` is set), and the reason there are none, as a
 * strings key and its arguments: `model-info-fetched` `[key]`,
 * `model-profile-no-model` `[id]`, `model-info-missing` `[path]`,
 * `model-info-unreadable` `[path]`.
 */
export type ModelInfo = {
  model?: string | null
  info?: ModelFile | null
  reason?: { key?: string; args?: (string | number)[] } | null
}

/**
 * What the route answers for a test that says nothing about the model file: no
 * layer counts, the file missing. The page then shows
 * `model-profile-model-facts-none` and keeps brief 14's weight fields
 * (brief 19 decision 2: blocks need a positive block count), so every GD1 and
 * GD2 test is as it was.
 */
export const MODEL_INFO: ModelInfo = {
  model: MODEL_FILE,
  info: null,
  reason: { key: 'model-info-missing', args: [MODEL_FILE] },
}

/** One model profile as GET /api/profiles/model lists it (fe/briefs/api-contract.md). */
export type ModelProfile = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  /** The section's `model` value as the file holds it; "" when unset (2b). */
  model: string
}

/** The default model profile list: one switched on, one switched off, one engine. */
export const MODEL_PROFILES: ModelProfile[] = [
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
 * One server profile as GET /api/profiles/server lists it
 * (fe/briefs/api-contract.md, the proposed section). Only `id` and `tags` are
 * named: the tags are all the tag suggestion reads (brief 15 C2).
 */
export type ServerProfile = {
  id: string
  tags: string[]
  engine?: string
  port?: number
  instance?: string | null
}

/** The default server profile list: the `default` profile, with no tags. */
export const SERVER_PROFILES: ServerProfile[] = [{ id: 'default', tags: [] }]

/**
 * One model profile as GET /api/profiles/model/{id} reads it (fe/briefs/api-contract.md,
 * the proposed section, the shape of criterion 81). `values` adds to or
 * replaces the keys of the section; `extra` adds, replaces, and with
 * `undefined` drops top-level members (of `revision`, of `id`, of anything:
 * the tests of 2d.66 need to answer without a key of the shape).
 */
export function modelProfileDoc(
  values: Record<string, string> = {},
  extra: Record<string, unknown> = {},
) {
  return {
    id: 'Qwen3-27B',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    values: {
      model: '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf',
      'spec-type': 'ngram-mod,draft-mtp',
      'ctx-size': '172032',
      ...values,
    },
    projectUrl: 'https://huggingface.co/org/repo',
    revision: 'r-1731628800-42',
    ...extra,
  }
}

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

/**
 * One server profile's document as GET /api/profiles/server/{id} reads it
 * (brief 16, 17a.5): the server document of the instance that runs the profile,
 * plus `instance`, the instance that runs it, and `null` when none does.
 */
export function serverProfileDoc(
  values: Record<string, unknown> = {},
  extra: Record<string, unknown> = {},
) {
  return serverDoc(values, { instance: 'default', ...extra })
}

type Answer = Response | ((init?: RequestInit) => Response | Promise<Response>)

export type BffOptions = {
  /** GET /api/session: `{"user":"alice"}`, 401 signed out, or the reply. Default signed in. */
  session?: 'signed-in' | 'signed-out' | Answer
  /** POST /api/session: the reply. Default 200 `{"user":"alice"}`. */
  signIn?: Answer
  /** DELETE /api/session: the reply. Default 204. */
  signOut?: Answer
  /**
   * GET /api/version (brief 18 section 0): the body to answer 200 with, or the
   * reply. Default 200 `{"version": "2.99.0", "os": {...}}`: the Overview's
   * version line, and the operating system About shows (AN.3). It carries no
   * `name` on purpose: a default host name would red every test that finds the
   * nav's label at its fallback "Instances" (13 N.4), so a test that wants the
   * host names one (`VERSION_HOST`).
   */
  version?: VersionBody | Record<string, unknown> | Answer
  /**
   * GET /api/node (fe/briefs/13-node-about.md section 0): the node to answer
   * 200 with, or the reply. A test names one (`NODE`, the brief's reply, is
   * exported for it); where it names none the route rejects as an unknown one
   * does, so a test that never reads the host finds the labels at their
   * fallback "Instances" (13 N.4).
   */
  node?: NodeInfo | Record<string, unknown> | Answer
  /**
   * GET /api/node/devices (brief 14 section 0, fe/briefs/api-contract.md): the
   * probe to answer 200 with, or the reply. A test names one (`DEVICES`, the
   * brief's reply, is exported for it). Where it names none the route rejects as
   * an unknown one does, so a page that reads the GPUs sees GD1.7's failure and
   * every other card stays as today (brief 14, section 6, decision 5).
   */
  devices?: DeviceProbe | Record<string, unknown> | Answer
  /**
   * GET /api/profiles/model/{id}/model-info (brief 19 section 0): the facts body
   * to answer 200 with, or the reply. A test names one (`MODEL_INFO`, no layer
   * counts, is exported for it). Where it names none the route answers
   * `MODEL_INFO`, so a page that says nothing about the model file keeps brief
   * 14's weight fields and its layer fields as they are (brief 19, section 6).
   */
  modelInfo?: ModelInfo | Record<string, unknown> | Answer
  /** GET /api/instances: the instances, or the reply. Default INSTANCES. */
  instances?: Instance[] | Answer
  /** GET /api/profiles/model: the model profiles, or the reply. Default MODEL_PROFILES. */
  modelProfiles?: ModelProfile[] | Answer
  /**
   * GET /api/profiles/server (brief 15 C2): the server profiles, or the reply.
   * Default SERVER_PROFILES, the `default` profile with no tags, so a test that
   * says nothing suggests nothing.
   */
  serverProfiles?: ServerProfile[] | Answer
  /**
   * GET /api/profiles/model/{id}: the reply per decoded id. Default 200 modelProfileDoc
   * with the `id` of the request (the file spells it as asked).
   */
  modelProfile?: (
    id: string,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * PUT /api/profiles/model/{id}/enabled: the reply to the n-th switch (from 1),
   * given the parsed body and the decoded id. Default 200 with the body's
   * `enabled` echoed and `running: false`.
   */
  modelProfileSwitch?: (
    body: unknown,
    id: string,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * PUT /api/profiles/model/{id}: the reply to the n-th save (from 1), given the
   * parsed body and the decoded id. Default 200 {"revision": "r2",
   * "running": false} (fe/briefs/api-contract.md).
   */
  modelProfileSave?: (
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
   * GET /api/profiles/server/{id} (brief 16, 17a.6): the reply per decoded id.
   * Default 200 `serverProfileDoc()`, the server document with
   * `"instance": "default"`.
   */
  serverProfile?: (
    id: string,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * PUT /api/profiles/server/{id} (brief 16, 17a.7): the reply to the n-th save
   * of the profile the page shows (from 1), given the parsed body. The shape of
   * `save`, whose replies these tests reuse: the id is the one the test opened.
   * Default 200 {"revision": "r-2", "running": false}.
   */
  serverProfileSave?: (
    body: unknown,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * POST /api/profiles/server (brief 16, 17b.4): the reply to the n-th create
   * (from 1), given the parsed body `{"engine","id","port"}`.
   * Default 201 `{"id": <the body's id>}`, the reply the page goes to.
   */
  serverProfileCreate?: (
    body: unknown,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
  /**
   * DELETE /api/profiles/server/{id} (brief 16, 17b.8): the reply to the n-th
   * delete (from 1) of the decoded id. Default 204.
   */
  serverProfileDelete?: (
    id: string,
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
  /**
   * PUT /api/instances/{id}/tags: the reply to the n-th save of the instance's
   * own tags (from 1), given the parsed body and the decoded id. Brief 12
   * (fe/briefs/api-contract.md): the body is `{"tags": [...]}`, the answer
   * 204. Default 204.
   */
  instanceTags?: (
    body: unknown,
    id: string,
    n: number,
    init?: RequestInit,
  ) => Response | Promise<Response>
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
      const s = options.session ?? 'signed-in'
      if (s === 'signed-in')
        return Promise.resolve(reply(200, { user: 'alice' }))
      if (s === 'signed-out')
        return Promise.resolve(
          reply(401, { key: 'session-required', args: [] }),
        )
      return answer(s, init)
    }
    if (route === 'POST /api/session')
      return answer(
        options.signIn ?? (() => reply(200, { user: 'alice' })),
        init,
      )
    if (route === 'DELETE /api/session')
      return answer(options.signOut ?? (() => reply(204)), init)
    if (route === 'GET /api/version') {
      const v = options.version
      // An object is the body of the 200, as the instances array and the node
      // are; the default carries no host name (see `version` above).
      if (v === undefined)
        return Promise.resolve(reply(200, { version: '2.99.0', os: OS }))
      if (typeof v === 'function' || v instanceof Response)
        return answer(v, init)
      return Promise.resolve(reply(200, v))
    }
    if (route === 'GET /api/node') {
      const n = options.node
      // A test that never asked about the host gets the rejection of an unknown
      // route, so its nav label and its heading stay "Instances" (13 N.4).
      if (n === undefined)
        return Promise.reject(new TypeError(`unexpected ${route}`))
      // An object is the body of the 200, as the instances array is.
      if (typeof n === 'function' || n instanceof Response)
        return answer(n, init)
      return Promise.resolve(reply(200, n))
    }
    if (route === 'GET /api/node/devices') {
      const d = options.devices
      // A test that never asked about the GPUs gets the rejection of an unknown
      // route: the page sees GD1.7's failure, and every other card is as today.
      if (d === undefined)
        return Promise.reject(new TypeError(`unexpected ${route}`))
      if (typeof d === 'function' || d instanceof Response)
        return answer(d, init)
      return Promise.resolve(reply(200, d))
    }
    if (route === 'GET /api/instances') {
      const i = instances ?? INSTANCES
      if (Array.isArray(i)) return Promise.resolve(reply(200, i))
      return answer(i, init)
    }
    if (route === 'GET /api/profiles/model') {
      const p = options.modelProfiles ?? MODEL_PROFILES
      if (Array.isArray(p)) return Promise.resolve(reply(200, p))
      return answer(p, init)
    }
    if (route === 'GET /api/profiles/server') {
      const p = options.serverProfiles ?? SERVER_PROFILES
      if (Array.isArray(p)) return Promise.resolve(reply(200, p))
      return answer(p, init)
    }
    if (route === 'POST /api/profiles/server') {
      const n = calls.filter(
        (c) => c.method === 'POST' && c.path === '/api/profiles/server',
      ).length
      return Promise.resolve(
        options.serverProfileCreate
          ? options.serverProfileCreate(body, n, init)
          : reply(201, { id: (body as { id?: string })?.id ?? '' }),
      )
    }
    const serverProfileDelete =
      /^DELETE \/api\/profiles\/server\/([^/]+)$/.exec(route)
    if (serverProfileDelete) {
      const id = decodeURIComponent(serverProfileDelete[1]!)
      const n = calls.filter(
        (c) => c.method === 'DELETE' && c.path === path,
      ).length
      return Promise.resolve(
        options.serverProfileDelete
          ? options.serverProfileDelete(id, n, init)
          : reply(204),
      )
    }
    const modelInfo = /^GET \/api\/profiles\/model\/([^/]+)\/model-info$/.exec(
      route,
    )
    if (modelInfo) {
      // Only a test that names no `modelInfo` gets the default: a test that
      // names a literal `null`, an array or a string body means that body, so
      // the shape checks of 19 G.1 are testable.
      const m = options.modelInfo === undefined ? MODEL_INFO : options.modelInfo
      // An object is the body of the 200, as the device probe's is.
      if (typeof m === 'function' || m instanceof Response)
        return answer(m, init)
      return Promise.resolve(reply(200, m))
    }
    const modelProfileSwitch =
      /^PUT \/api\/profiles\/model\/([^/]+)\/enabled$/.exec(route)
    if (modelProfileSwitch) {
      const id = decodeURIComponent(modelProfileSwitch[1]!)
      const n = calls.filter(
        (c) =>
          c.method === 'PUT' &&
          /^\/api\/profiles\/model\/[^/]+\/enabled$/.test(c.path),
      ).length
      const enabled = (body as { enabled?: unknown } | undefined)?.enabled
      return Promise.resolve(
        options.modelProfileSwitch
          ? options.modelProfileSwitch(body, id, n, init)
          : reply(200, { enabled, running: false }),
      )
    }
    const modelProfileSave = /^PUT \/api\/profiles\/model\/([^/]+)$/.exec(route)
    if (modelProfileSave) {
      const id = decodeURIComponent(modelProfileSave[1]!)
      const n = calls.filter(
        (c) => c.method === 'PUT' && c.path === path,
      ).length
      return Promise.resolve(
        options.modelProfileSave
          ? options.modelProfileSave(body, id, n, init)
          : reply(200, { revision: 'r2', running: false }),
      )
    }
    const modelProfileRead = /^GET \/api\/profiles\/model\/([^/]+)$/.exec(route)
    if (modelProfileRead) {
      const id = decodeURIComponent(modelProfileRead[1]!)
      return Promise.resolve(
        options.modelProfile
          ? options.modelProfile(id, init)
          : reply(200, modelProfileDoc({}, { id })),
      )
    }
    const serverProfileRead = /^GET \/api\/profiles\/server\/([^/]+)$/.exec(
      route,
    )
    if (serverProfileRead) {
      const id = decodeURIComponent(serverProfileRead[1]!)
      return Promise.resolve(
        options.serverProfile
          ? options.serverProfile(id, init)
          : reply(200, serverProfileDoc()),
      )
    }
    const serverProfileSave = /^PUT \/api\/profiles\/server\/([^/]+)$/.exec(
      route,
    )
    if (serverProfileSave) {
      const n = calls.filter(
        (c) => c.method === 'PUT' && c.path === path,
      ).length
      return Promise.resolve(
        options.serverProfileSave
          ? options.serverProfileSave(body, n, init)
          : reply(200, { revision: 'r-2', running: false }),
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
    const instanceTags = /^PUT \/api\/instances\/([^/]+)\/tags$/.exec(route)
    if (instanceTags) {
      const id = decodeURIComponent(instanceTags[1]!)
      const n = calls.filter(
        (c) => c.method === 'PUT' && c.path.endsWith('/tags'),
      ).length
      return Promise.resolve(
        options.instanceTags
          ? options.instanceTags(body, id, n, init)
          : reply(204),
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

/**
 * A test whose work is long in wall clock: it drives the page's polls across
 * minutes of one-second fake-clock steps, opens the whole app many times, or
 * pays the app's first cold render of the file. On a loaded machine that is
 * far past vitest's 5 s default, and a test that times out is not cancelled:
 * it keeps running and its clean-up unmounts the tree of the tests that
 * follow it. The timeout here is at least twice the worst case measured for
 * these tests under load (comments keep no measured figures).
 */
export function heavyTest(name: string, fn: () => void | Promise<void>) {
  test(name, { timeout: 40_000 }, fn)
}
