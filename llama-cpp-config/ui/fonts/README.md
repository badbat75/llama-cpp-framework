# ui\fonts\

`FluentSystemIcons-Regular.ttf` is the icon font of the configurator: Microsoft's [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons), MIT licensed (`LICENSE`, copied from that repository). `components.slint` imports it, which compiles it into the binary, so every icon renders the same on Windows and on Linux with nothing installed.

- **Unmodified and whole**, not subset: `fonts/FluentSystemIcons-Regular.ttf` at upstream commit `a347af95478f989038cccbf085978865264c048a` (2026-10-09). sha256 `f977ae5bfb46661e7ba94b098e0d7720e27af888e57b9b14e239933ac4c0e6f4`.
- **Family name**: `FluentSystemIcons-Regular`, which is what `Icons.font` in `components.slint` must say.
- **Codepoints** are private-use values specific to this font. Upstream's `fonts/FluentSystemIcons-Regular.json` maps each icon name (`ic_fluent_info_16_regular`) to its codepoint in DECIMAL; `Icons` writes them in hex. The font is regenerated upstream and a codepoint is not guaranteed stable across commits: after replacing the file, re-check every entry of `Icons` against the new json and look at the window.
- Each icon exists per size (`_16_`, `_20_`, ...), drawn for that size: pick the one nearest the `font-size` it is shown at.
