# Windows executable icon

The portable kea.exe embeds a multi-resolution icon; no installer, signature,
administrator access or external image file is needed for Explorer to display
it. This does not bypass SmartScreen or application-control policy.

`assets/windows/kea.ico` packages the existing Kea PNG artwork at 16, 24, 32,
48, 64, 128 and 256 pixels. Regenerate with
`python scripts/generate-windows-icon.py`; CI uses `--check` to reject drift.
`kea-app/build.rs` compiles the Windows resource into the kea binary only and
treats resource compilation failures as build failures. Windows CI inspects
the actual PE resource group and compares every image with the source ICO.

Explorer/Start shortcuts use the executable's icon. Pinned shortcuts to an old
path or an explicitly overridden icon can still show an old image; recreate
that shortcut after replacing the executable. Runtime taskbar/Alt-Tab and
Start-menu appearance must also be checked on Windows, not inferred from a
Linux build or resource-file existence alone.
