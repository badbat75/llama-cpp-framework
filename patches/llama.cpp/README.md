# Out-of-tree patches applied to the llama.cpp working tree

`02-build.ps1` applies every `*.patch` in this directory to `$cfg.LlamaCppDir`
(`build\llama.cpp`) in file-name order, right after it detaches the clone onto
the newest `bNNNN` release tag, and **reverts them again before the next
fetch/checkout** so the checkout always runs on a clean tree (a tag bump that
touches a patched file would otherwise abort with "local changes would be
overwritten"). Both legs probe with `git apply --check` first, so re-running
the build is a no-op rather than an error, and a patch that no longer applies
**fails the build fast** instead of being skipped silently: an out-of-tree
change that quietly stops being applied is a binary that behaves differently
from the one the last build produced, with nothing in the log saying so.

Patch files are stored **verbatim** as published upstream, never reflowed or
"improved": a refresh is then a plain diff against the new upstream file, and
the checked-in sha256 below is the one the publisher serves. `.gitattributes`
pins `*.patch` to LF for the same reason (a CRLF checkout of the patch does not
apply to LF sources).

The llama.cpp clone lives under the gitignored `build\`, so nothing here is
committed into it; the patch is re-applied from this directory on every build.

## `0001-qwen35-fastmtp-d2t.patch`

Teaches the `qwen35` arch (Qwen3.5 / Qwen3.8) to read a **trimmed draft
vocabulary** from a standalone MTP head GGUF, which is what HauhauCS's
"FastMTP" sidecar ships. Source:

- <https://huggingface.co/HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF/resolve/main/HauhauCS-FastMTP-llama.cpp.patch>
- sha256 `981285400b59dc45cf99936b6ff66d4b3aa0f1b532f85fa51418cb407e51d615`
- published against upstream `4df29be4f4c3673f428170fda944a5b19f743bb8`;
  verified to apply and compile clean on tag **b10488** (2026-08-20)

**What it changes.** A FastMTP sidecar is an MTP-only GGUF (no trunk blocks)
whose LM head covers **32,768** rows instead of the target's 248,320, plus a
`d2t` tensor (I64) mapping those rows onto target token ids. Stock
`src\models\qwen35.cpp` sizes `output.weight` as `{n_embd, n_vocab}` and so
rejects the file at load with `expected 5120, 248320, got 5120, 32768`. The
patch reads `d2t`'s length as `n_vocab_out`, sizes the head to it, and, in
`graph_mtp`, scatters the draft logits back onto the full vocabulary
(`ggml_fill` to `-INFINITY` then `ggml_set_rows` through `d2t`) so the sampler
and the verify step see ordinary full-width logits.

**Blast radius.** Both legs are gated on `model.d2t`, which is created only for
an `mtp_only` file that actually carries a `d2t` tensor. A normal target GGUF
with embedded MTP heads, and every other arch, take byte-identical paths. `d2t`
itself is not new: it is an existing `llama_model` member that `dflash.cpp` and
`eagle3.cpp` already load and scatter through in exactly this shape; this patch
is that same code on the third arch that needed it.

**The one divergence from the upstream copies is load-bearing, not sloppiness.**
`dflash.cpp` and `eagle3.cpp` both assert `model.d2t->type == GGML_TYPE_I64`
before the scatter; this patch asserts only the length, and it has to. The
FastMTP sidecar ships `d2t` as **I32** (it is the file's single `i32` tensor),
which `ggml_set_rows` accepts (`ggml.c`: `c->type == GGML_TYPE_I64 ||
c->type == GGML_TYPE_I32`) but the upstream assert would reject. So do not
"restore" that assert when refreshing the patch: it would turn the one file this
exists for into a hard failure.

**Confirming it is live** (rather than assuming): the loader logs
`QWEN35 MTP using d2t draft-vocab trim (n_vocab_out = 32768)`, at llama.cpp's
INFO level, which llama-server prints only from `-lv 4` up (`LogVerbosity` in
`server.ini`); at the usual 3 the line is absent and proves nothing. The failure
mode without the patch is a refused load
(`expected 5120, 248320, got 5120, 32768`), not a slow or silently
non-speculating server, so a server that starts at all with the sidecar attached
is already evidence.

Verified end to end on b10488 (2026-08-20) against
`Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf` +
`...-FastMTP-32K.gguf`: the trim line appears, an 8-token completion comes out
coherent, and llama-server reports `draft acceptance = 0.57143 (4 accepted /
7 generated), mean len = 2.33`, i.e. the drafts are real and accepted rather
than garbage the target quietly rejects.

**Using it from llama-cpp-config**: a per-model preset with
`model-draft = <...FastMTP-32K.gguf>`, `spec-type = draft-mtp` and
`spec-draft-n-max = 3` (the depth the publisher qualified; the stock embedded
head runs at 2). Because the sidecar is a **separate draft file**, unlike the
embedded MTP heads, `device-draft` / `n-gpu-layers-draft` are honoured here and
should pin the sidecar to the same device as the model. `--spec-draft-p-min`
has no preset field; llama.cpp's default applies.

**When to drop it.** If upstream lands the same support, `git apply --check`
starts failing and the build stops with a message pointing here: delete the
file (git history keeps it) and rebuild. Re-verify at every llama.cpp tag bump
that touches `src\models\qwen35.cpp`; a refreshed patch from the publisher
replaces this one wholesale, and the sha256 above with it.
