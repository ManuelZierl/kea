#!/usr/bin/env python3
"""Verify actual PE icon resources, not Windows' generic fallback icon.

Uses Win32's resource loader as a data file, never executes the application.
Every embedded image must match the committed source ICO byte for byte.
"""
import ctypes
from ctypes import wintypes
from pathlib import Path
import struct
import sys

def main() -> None:
    if sys.platform != "win32":
        raise SystemExit("Run this verifier on Windows")
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify-windows-icon.py path/to/kea.exe")
    path = Path(sys.argv[1]).resolve(strict=True)
    ico = (Path(__file__).resolve().parents[1] / "assets/windows/kea.ico").read_bytes()
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HRSRC
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel.LoadResource.restype = wintypes.HGLOBAL
    kernel.LockResource.argtypes = [wintypes.HGLOBAL]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    kernel.FreeLibrary.restype = wintypes.BOOL
    module = kernel.LoadLibraryExW(str(path), None, 0x00000002 | 0x00000020)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    def resource(kind: int, identity: int) -> bytes:
        handle = kernel.FindResourceW(module, identity, kind)
        if not handle:
            raise RuntimeError(f"Missing PE resource type={kind}, id={identity}")
        size = kernel.SizeofResource(module, handle)
        loaded = kernel.LoadResource(module, handle)
        pointer = kernel.LockResource(loaded) if loaded else None
        if not size or not pointer:
            raise ctypes.WinError(ctypes.get_last_error())
        return ctypes.string_at(pointer, size)
    try:
        group = resource(14, 1) # RT_GROUP_ICON
        reserved, kind, count = struct.unpack_from("<HHH", group)
        if (reserved, kind, count) != struct.unpack_from("<HHH", ico) or count != 7:
            raise RuntimeError("Wrong application icon group")
        for n in range(count):
            actual = struct.unpack_from("<BBBBHHIH", group, 6 + n * 14)
            expected = struct.unpack_from("<BBBBHHII", ico, 6 + n * 16)
            if actual[:7] != expected[:7]:
                raise RuntimeError(f"Incorrect icon entry {n}")
            payload = resource(3, actual[7]) # RT_ICON
            if payload != ico[expected[7]:expected[7] + expected[6]]:
                raise RuntimeError(f"Embedded icon payload {n} differs from source")
        print("Verified Kea executable icon: seven exact embedded image resources")
    finally:
        kernel.FreeLibrary(module)

if __name__ == "__main__":
    main()
