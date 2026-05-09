# Relocate this Python venv to its current directory.
#
# Rewrites the shebang in every Scripts/*.exe launcher (open-webui.exe, pip.exe,
# etc.) to point at this venv's own python.exe, and updates pyvenv.cfg's
# executable + command lines. Run this via the venv's own Scripts/python.exe
# after the venv has been copied/installed to a new location on Windows.
#
# pyvenv.cfg's "home" is left untouched: it points at the base Python install
# the venv was created against, which the venv launcher needs to load python3X.dll.
# Full cross-machine portability would also require bundling base Python.
from __future__ import annotations

import sys
from pathlib import Path

ZIP_MAGIC = b"PK\x03\x04"


def patch_launcher(exe: Path, new_python: str) -> str:
    data = exe.read_bytes()
    zip_pos = data.find(ZIP_MAGIC)
    if zip_pos == -1:
        return "skip:no-zip"
    shebang_start = data.rfind(b"#!", 0, zip_pos)
    if shebang_start == -1 or zip_pos - shebang_start > 1024:
        return "skip:no-shebang"
    new_shebang = b"#!" + new_python.encode("utf-8") + b"\r\n"
    new_data = data[:shebang_start] + new_shebang + data[zip_pos:]
    if new_data == data:
        return "ok"
    exe.write_bytes(new_data)
    return "patched"


def main() -> int:
    venv_root = Path(__file__).parent.resolve()
    scripts = venv_root / "Scripts"
    if not scripts.is_dir():
        print(f"No Scripts/ at {venv_root}", file=sys.stderr)
        return 1

    new_python = str(scripts / "python.exe")
    print(f"Relocating venv at {venv_root}")
    print(f"  shebang -> {new_python}")

    cfg = venv_root / "pyvenv.cfg"
    if cfg.exists():
        out = []
        for line in cfg.read_text().splitlines():
            key = line.split("=", 1)[0].strip().lower() if "=" in line else ""
            if key == "executable":
                out.append(f"executable = {new_python}")
            elif key == "command":
                out.append(f"command = {new_python} -m venv --clear {venv_root}")
            else:
                out.append(line)
        cfg.write_text("\n".join(out) + "\n")
        print("  pyvenv.cfg updated")

    patched = skipped = 0
    for exe in sorted(scripts.glob("*.exe")):
        if exe.name in ("python.exe", "pythonw.exe"):
            continue
        result = patch_launcher(exe, new_python)
        if result == "patched":
            patched += 1
        else:
            skipped += 1
    print(f"  patched {patched} launchers, skipped {skipped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
