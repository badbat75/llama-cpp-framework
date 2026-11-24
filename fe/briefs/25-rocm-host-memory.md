# Brief 25: ROCm host memory as GTT on the Server page (slice GT2)

User, 2026-11-24. The design is `ARCHITECTURE.md` "ROCm host memory as GTT". Slice GT1 built the
server profile key `RocmHostMemoryGtt` (bool). When it is unset or `true`, the launch exports
`HSA_USERPTR_FOR_PAGED_MEM=0` and `HSA_SVM_FOR_PAGED_MEM=0` to llama-server. When it is `false`, the
launch exports neither. The bff reads and saves the key, and refuses a value that is not a bool with
422 and `field`. This slice adds it as a field of the Server form's "SDK tuning" card, so the Server
page and every server profile page get it.

## The field

It follows the `Webui` pattern (`fe/src/ServerForm.tsx`), the form's other key that is on by default:
a checkbox, checked when the key is unset or `true`, saved as `true` or `false`. It is not the
hipBLASLt default/on/off select: that select exists because "default" there leaves the choice to
rocBLAS, while here the default is plainly on. The GUI (`crates/config/src/server_form.rs`) keeps a
tri-state with no widget yet. It differs on purpose: the web form writes `true` where the GUI would
leave the key unset, and both mean the same launch.

## Criteria (GT2.1 to GT2.5)

**GT2.1 Place.** The field is a `CheckboxField` with id `${fid}-RocmHostMemoryGtt`, in the ROCm
subgroup of the SDK tuning card. It comes after hipBLASLt and before Profiler attach, so the
order is hipBLASLt, Host memory as GTT, Profiler attach.

**GT2.2 Words** (new keys in `fe/src/strings/en_US.json`):
- `server-rocm-gtt`: `Host memory as GTT` (the label)
- `server-rocm-gtt-toggle`: `keep ROCm's host memory in GTT` (the checkbox's text)
- `server-rocm-gtt-hint`: `Puts the memory ROCm takes from system RAM in the graphics driver's GTT
  pool, so the kernel no longer moves it while llama-server runs. Without this, memory compaction
  and reclaim can stall the GPU in the middle of a request.\nTurn it off if loading a model fails
  after this was switched on: GTT memory cannot be swapped out, and GTT is capped near half the
  RAM, so a large allocation in system memory can fail where it used to succeed.\nChecked = set both
  variables to 0 (the default); unchecked = don't set them, so ROCm's own defaults apply.\nNo effect
  on CUDA, Vulkan or Metal builds.\nVariables: HSA_USERPTR_FOR_PAGED_MEM=0,
  HSA_SVM_FOR_PAGED_MEM=0`
  The `\n` breaks follow the other hints of the card (each starts a paragraph: `fe/DESIGN.md`, Hint).

**GT2.3 Default shown.** A document whose `values.RocmHostMemoryGtt` is `null` or missing shows the
checkbox checked, and so does one with `true`. One with `false` shows it unchecked.

**GT2.4 Save.** The save's `values` carry `RocmHostMemoryGtt`: `true` when checked and `false` when
unchecked, as `Webui` is saved. Unchecking marks the form dirty, and Discard puts it back. A 422
with `field: "RocmHostMemoryGtt"` shows its text as the field's error under the checkbox
(`aria-describedby`, the form's error pattern), and the first error takes the focus, as for every
field ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification)).

**GT2.5 Accessibility.** The checkbox has the label as its accessible name and the hint as its
description, as the card's other checkboxes do
([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
[4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). Space toggles it, and it sits in the tab
order between hipBLASLt and Profiler attach ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). It
adds no new component, token or style.

Out of scope: the GUI widget in `pai-config`, a check of the backend (the hint says which builds
ignore it), and the docs.

Tests to extend: `fe/src/ServerSaveKeys.test.tsx` (the key in the saved values, both states) and
`fe/src/ServerSave.test.tsx` (the place, the default shown, the 422 field). If the test fixtures
(`fe/src/test/bff.ts`) list the server document's keys, the key is added there as `null`.
