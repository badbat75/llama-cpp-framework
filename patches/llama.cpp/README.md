# Patches applied to llama.cpp

`xtask build` checks out the newest `vX.Y.Z` tag in `build/<os>/llama.cpp` and applies every
`*.patch` here in file-name order. Before a checkout that moves to another tag it reverts them;
whatever the revert leaves behind is stashed in the clone (`git stash list`), never discarded.

**A patch that no longer applies stops the build.** Either refresh it for the new tag or delete
it. Refresh with `git apply --3way` on a clean checkout of the tag, and update its sha256 here.

| Patch | For | Upstream |
| --- | --- | --- |
| `0001-qwen35-fastmtp-d2t` | Loads MTP sidecars with a trimmed vocabulary (FastMTP) on `qwen35` | No equivalent |
| `0005-qwen35-mtp-graph-realloc-pr31415` | Stops draft reallocations after an image on `qwen35` | PR #31415, open |
| `0006-qwen4exp-mtp-graph-realloc` | The same fix on `qwen4exp` (Qwen3.8-Flash-Next) | Our port, not published |

`0002`, `0003` and `0004` were dropped to reduce upkeep, and performance has dropped since. Whether to bring
them back is open: see "Performance regression since patches 0002 to 0004 were dropped" in
[TODO.md](../../TODO.md).

## `0001-qwen35-fastmtp-d2t`

sha256 `34db7d82eeb458a6c1e31ec80c2a83c047b484c7b6d2792be32cf75a109c5c91`. Derived from
[HauhauCS's patch](https://huggingface.co/HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF/resolve/main/HauhauCS-FastMTP-llama.cpp.patch),
refreshed locally, so it is no longer the publisher's bytes.

**What it does.** A FastMTP sidecar is an MTP-only GGUF whose head covers fewer rows than the
target's vocabulary, plus a `d2t` tensor mapping them onto target token ids. Stock `qwen35.cpp`
refuses it at load (`expected 5120, 248320, got 5120, 32768`). The patch sizes the head from `d2t`
and scatters the draft logits back onto the full vocabulary. Only a file that carries `d2t` takes
the new path.

**Keep when refreshing:**

- The load-time checks, which the publisher's version lacks. `ggml_set_rows` writes wherever an
  index points, so a `d2t` of the wrong type, of the wrong length or with an entry outside
  `[0, n_vocab)` is refused.
- I32 accepted alongside I64. `dflash.cpp` and `eagle3.cpp` assert I64, but the FastMTP sidecar
  ships I32, so restoring that assert rejects the one file this patch exists for.

**Using it.** A preset with `model-draft = <...FastMTP-32K.gguf>`, `spec-type = draft-mtp` and
`spec-draft-n-max = 3`, the depth the publisher qualified. `device-draft` and `n-gpu-layers-draft`
apply to a separate draft file: pin the sidecar to the model's device.

**Confirming it is live.** At `-lv 4` the loader logs `QWEN35 MTP using d2t draft-vocab trim`.
Without the patch the load fails, so a server that starts with the sidecar is already evidence.

**Drop it** when upstream supports `d2t` in `qwen35.cpp`.

## `0005-qwen35-mtp-graph-realloc-pr31415`

sha256 `9345730ea1f76a5eaaf433f441d9cfe902a5bbd91b6c1982a0e2bba1c633fe4b`.
[PR #31415](https://github.com/ggml-org/llama.cpp/pull/31415) (commit `dc704b74`) as GitHub serves
it, 3-way refreshed for the bundled tag: upstream moved the `graph_mtp` constructor and the
`inp->h` line around it, the fix itself is the PR's. Fixes our issue
[#31302](https://github.com/ggml-org/llama.cpp/issues/31302). Applies after `0001`.

**What it does.** `graph_mtp` chose its input with an `if` on `ubatch.token`, so tokens and image
rows built two different graphs. After the first image the draft context switches between them on
every draft cycle and the scheduler re-reserves twice per cycle, for the rest of the server's life.
The patch builds both inputs and picks one with `ggml_build_forward_select`.

**Measured** (2x R9700, Swift-1.5-Qwen3.8-27B Q8_0, 2026-10-30). Draft reallocations over text
requests, an image and more text: 0 / 38 / 267 without the patch, 0 with it. Decode, response text
and draft acceptance unchanged: it removes the churn and brings no speed at this length.

**Confirming it is live.** At `-lv 4`, an image request followed by a text request logs no
`sched_graph_realloc` line with a two-digit node count.

**Drop it** when the bundled release contains #31415. If the PR changes first, download it again.

## `0006-qwen4exp-mtp-graph-realloc`

sha256 `b411f05246ae67544d87ab5d1fbff25debf579c21de0ec80252cb88deb4fb4b5`. Our port of `0005` to
`src/models/qwen4exp.cpp`, with the same defect and the same fix, refreshed for the bundled tag
the same way as `0005`.

**Measured** (2x R9700, Swift-Qwen3.8-Flash-Next IQ3_S, 2026-11-01). Draft reallocations over 2
text requests, an image and 3 more: 0 / 0 / 408 / 336 / 338 / 332 without the patch, 0 with it.
Decode, response text and draft acceptance unchanged. Only the path with the embedding in RAM ran:
the `ggml_dup` branch (`token_embd` on a GPU) is untested here.

**Confirming it is live.** At `-lv 4`, an image request followed by a text request logs no
`sched_graph_realloc` line after the load.

**Drop it** when the bundled release fixes `qwen4exp`. Merging #31415 as it stands does not.
