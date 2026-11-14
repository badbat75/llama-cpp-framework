---
name: release
description: Cut a framework release. Use when bumping the version, building and packaging every OS for a release, writing the release commit, tagging, creating the GitHub release or uploading its packages.
---

# Release process

One GitHub release per framework release carries every package of every OS: the framework's and llama.cpp's at the release the framework was built against (user, 2026-11-14, `PLAN.md` step 11). Each OS builds its own packages on its own machine, so the release is assembled from three `dist\` folders.

1. **Bump version** in the root `Cargo.toml` (`[workspace.package] version = "X.Y.Z"`, one version for `pai-core`, `pai-app`, `pai-manager`, `pai-config`, `pai-cli` and `pai-bff`). Until 3.0.0 every release is a pre-release of it numbered `2.99.N`: after 2.4.0 come 2.99.0, 2.99.1, 2.99.2...; 3.0.0 is the user's call (decided 2026-11-13). The next build also refreshes those crates' lines in the workspace's `Cargo.lock` (at the repo root): both files belong to the release commit. Raise `LLAMA_CPP_REVISION` in `xtask\src\common\components.rs` (the `+pai<N>` of the engine's Linux version) for any change that is not llama.cpp's code on the same llama.cpp release (the patches, the build options, the package's scripts or files), and set it back to 1 at every new llama.cpp release (user, 2026-11-14; its doc comment); a test pins its value, so the bump is a code change with that test. Only the Linux engine packages carry it: llama.cpp's MSI and `.pkg` are named and versioned by the release alone.
2. **Run `xtask build`** on each OS: with no product it builds both, llama.cpp (fetches and checks out the newest `vA.B.C` **release** tag; `bNNNN` nightlies are not candidates) and the framework. Nothing pins the tag across machines: check that every OS built the same release (the engine's file names show it).
3. **Run `xtask package`** on each OS: with no product it packages both into `dist\`. It refuses a llama.cpp clone not on a release tag, on every OS.
   - Windows: `private-ai-framework-vX.Y.Z-x64.msi` (ProductVersion the framework version) and `llama.cpp-vA.B.C-x64.msi` (ProductVersion the llama.cpp release), each validated (ICE) and refused at 2 GiB or more (GitHub's asset limit, `xtask\src\windows\package.rs`).
   - Linux: eight `.deb` and eight `.rpm`: `private-ai-framework-{config,cli,bff,core}-vX.Y.Z-<arch>`, `llama.cpp-vA.B.C+pai<N>-<arch>` (package version `1:A.B.C+pai<N>`) and the runtimes `llama.cpp-{rocm,cuda,vulkan}-v<sdkVersion>-<arch>`. No size check: the largest of the 2.99.1 build, `llama.cpp-rocm-v10.1.0-x86_64.rpm`, is 922 MiB (`dist/`, 2026-11-14).
   - macOS: five `.pkg`: `private-ai-framework-{config,cli,core}-vX.Y.Z-arm64.pkg`, `llama.cpp-vA.B.C-arm64.pkg`, `llama.cpp-vulkan-v<sdkVersion>-arm64.pkg`. No size check.
4. **Test the installers.** Windows: the installer test plan in [installer\AGENTS.md](installer/AGENTS.md) on a test machine whenever a `.wxs` or its scripts changed. Neither MSI has been built by WiX or run, and no `.pkg` built or run on a Mac, since the split: the first release after it needs both.
5. **Commit** all changes. Subject: **`Release vX.Y.Z: <summary>`**, the SAME first line as the tag (step 6); a `feat:`/`docs:`-style subject is not the convention for release commits. Body: a prose summary, closing with a **`Bundles llama.cpp <tag> (up from <previous tag>).`** line, e.g. `Bundles llama.cpp v0.2.0 (up from v0.1.2).`.
6. **Tag**: `git tag -a X.Y.Z -m "Release vX.Y.Z: <short summary>..." && git push origin X.Y.Z`
   - Tag name: **no `v` prefix** (e.g. `1.5.1`, not `v1.5.1`).
   - Tag message: starts with `Release vX.Y.Z: <summary>` on first line, then blank line, then bullet points of changes. Match the style of previous tags (`git show 1.5.0`).
7. **Create release**: `gh release create X.Y.Z --prerelease --title "vX.Y.Z (llama.cpp vA.B.C)" --notes "<markdown body>"`
   - `--prerelease` while the version is 2.99.N (the pre-releases of 3.0.0); 3.0.0 and later drop it.
   - Title format: **`vX.Y.Z (llama.cpp vA.B.C)`** (with `v` prefix; the llama.cpp release the packages were built against).
   - Body: detailed markdown with sections (`##`, `###`), bullet points, code formatting. Match the style of previous releases (`gh release view 1.5.0`). End with test count and the llama.cpp release.
8. **Upload every package**, from each machine's `dist\` (it keeps older builds: upload only this release's files):
   - Windows (PowerShell, no glob): `gh release upload X.Y.Z dist\private-ai-framework-vX.Y.Z-x64.msi dist\llama.cpp-vA.B.C-x64.msi`
   - Linux: `gh release upload X.Y.Z dist/private-ai-framework-*-vX.Y.Z-* dist/llama.cpp-vA.B.C+pai<N>-* dist/llama.cpp-{rocm,cuda,vulkan}-v*` (one file per runtime and format: remove older SDK versions first)
   - macOS: `gh release upload X.Y.Z dist/private-ai-framework-*-vX.Y.Z-arm64.pkg dist/llama.cpp-vA.B.C-arm64.pkg dist/llama.cpp-vulkan-v*-arm64.pkg`
   - A llama.cpp-only rebuild (`+pai<N+1>`, same release) goes out with the next framework release, or as re-uploaded assets of the current one (`gh release upload X.Y.Z <files> --clobber` overwrites assets of the same name).

**Formatting conventions** (older releases show a `bNNNN` llama.cpp token instead of `vA.B.C`, and 2.4.0 and earlier the `llama-cpp-framework-` installer prefix; until 3.0.0, `X.Y.Z` is `2.99.N`, e.g. tag `2.99.0`, title `v2.99.0 (llama.cpp vA.B.C)`):

| Artifact | Format | Example |
|---|---|---|
| Commit subject | `Release vX.Y.Z: <summary>` (matches the tag) | `Release v1.5.1: proxy/gateway support...` |
| Tag name | `X.Y.Z` (no `v`) | `1.5.1` |
| Tag message | `Release vX.Y.Z: <summary>\n\n- bullets...` | `Release v1.5.1: proxy/gateway support...` |
| Release title | `vX.Y.Z (llama.cpp vA.B.C)` | `v1.11.2 (llama.cpp v0.2.0)` |
| Windows assets | `private-ai-framework-vX.Y.Z-x64.msi`, `llama.cpp-vA.B.C-x64.msi` (before the split, one MSI: `private-ai-framework-vX.Y.Z-vA.B.C-x64.msi`) | `private-ai-framework-v2.99.1-x64.msi`, `llama.cpp-v1.0.0-x64.msi` |
| Linux assets | `private-ai-framework-<component>-vX.Y.Z-<arch>`, `llama.cpp-vA.B.C+pai<N>-<arch>`, `llama.cpp-<sdk>-v<sdkVersion>-<arch>`, each `.deb` (`amd64`) and `.rpm` (`x86_64`) | `private-ai-framework-core-v2.99.1-amd64.deb`, `llama.cpp-v1.0.0+pai1-x86_64.rpm`, `llama.cpp-rocm-v10.1.0-amd64.deb` |
| macOS assets | `private-ai-framework-<component>-vX.Y.Z-arm64.pkg`, `llama.cpp-vA.B.C-arm64.pkg`, `llama.cpp-vulkan-v<sdkVersion>-arm64.pkg` | `private-ai-framework-cli-v2.99.1-arm64.pkg`, `llama.cpp-v1.0.0-arm64.pkg` |
