"""Read-only probe of a running mGBA bridge for test-only live diagnostics.

This reads the signed ROM's documented bridge ABI from emulator process
memory. It does not alter the emulator, profile, save, or release fixture.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import struct
import time
from ctypes import wintypes
from pathlib import Path


class MemoryBasicInformation(ctypes.Structure):
    _fields_ = [
        ("BaseAddress", ctypes.c_void_p),
        ("AllocationBase", ctypes.c_void_p),
        ("AllocationProtect", wintypes.DWORD),
        ("_alignment", wintypes.DWORD),
        ("RegionSize", ctypes.c_size_t),
        ("State", wintypes.DWORD),
        ("Protect", wintypes.DWORD),
        ("Type", wintypes.DWORD),
        ("_padding", wintypes.DWORD),
    ]


KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
KERNEL32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
KERNEL32.OpenProcess.restype = wintypes.HANDLE
KERNEL32.VirtualQueryEx.argtypes = (
    wintypes.HANDLE, ctypes.c_void_p, ctypes.POINTER(MemoryBasicInformation), ctypes.c_size_t
)
KERNEL32.VirtualQueryEx.restype = ctypes.c_size_t
KERNEL32.ReadProcessMemory.argtypes = (
    wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
)
KERNEL32.ReadProcessMemory.restype = wintypes.BOOL
KERNEL32.CloseHandle.argtypes = (wintypes.HANDLE,)


def read_memory(handle: int, address: int, size: int) -> bytes:
    buffer = ctypes.create_string_buffer(size)
    read = ctypes.c_size_t()
    if not KERNEL32.ReadProcessMemory(handle, ctypes.c_void_p(address), buffer, size, ctypes.byref(read)):
        return b""
    return buffer.raw[: read.value]


def find_bridge(handle: int, manifest: dict) -> list[int]:
    bridge = manifest["net_bridge"]
    signature = struct.pack(
        "<IHHI", bridge["magic"], bridge["abi_version"],
        bridge["game_protocol_version"], manifest["game_build"]["numeric_id"],
    )
    matches: list[int] = []
    address = 0
    maximum = 0x7FFF_FFFF_FFFF
    while address < maximum:
        region = MemoryBasicInformation()
        if not KERNEL32.VirtualQueryEx(handle, ctypes.c_void_p(address), ctypes.byref(region), ctypes.sizeof(region)):
            break
        base = int(region.BaseAddress or 0)
        end = base + int(region.RegionSize)
        if end <= address:
            break
        readable = region.State == 0x1000 and (region.Protect & 0xFF) in (
            0x02, 0x04, 0x08, 0x20, 0x40, 0x80,
        )
        if readable:
            cursor = base
            while cursor < end:
                size = min(1024 * 1024, end - cursor)
                data = read_memory(handle, cursor, size)
                if data:
                    offset = data.find(signature)
                    while offset >= 0:
                        matches.append(cursor + offset)
                        offset = data.find(signature, offset + 1)
                cursor += size - len(signature) + 1 if size > len(signature) else size
        address = end
    return matches


def snapshot(handle: int, address: int, manifest: dict) -> dict:
    bridge = manifest["net_bridge"]
    data = read_memory(handle, address, bridge["size"])
    if len(data) != bridge["size"]:
        raise RuntimeError("bridge became unreadable")
    offsets = bridge["offsets"]
    queue = bridge["queue"]
    status = struct.unpack_from("<I", data, offsets["status_flags"])[0]
    result = {
        "host_address": hex(address),
        "status_flags": hex(status),
        "status": {
            name: bool(status & (1 << bit)) for bit, name in enumerate((
                "initialized", "rom_ready_sent", "session_ready", "player_state_sent",
                "queue_congested", "queue_error", "checksum_error", "sidecar_heartbeat_seen",
                "sidecar_heartbeat_stale", "world_not_ready", "protocol_error",
            ))
        },
        "sidecar_heartbeat": struct.unpack_from("<I", data, offsets["last_sidecar_heartbeat"])[0],
    }
    for name in ("game_to_network", "network_to_game"):
        base = offsets[name]
        read_index, write_index = struct.unpack_from("<HH", data, base)
        result[name] = {
            "read_index": read_index,
            "write_index": write_index,
            "count": (write_index - read_index) & 0xFFFF,
            "head_type": struct.unpack_from("<H", data, base + queue["entries_offset"]
                + (read_index % queue["capacity"]) * bridge["message"]["size"])[0]
                if read_index != write_index else None,
        }
    # The current paired ROMs place the private runtime 176 bytes before the
    # public bridge. This is diagnostic evidence only, not a portable ABI;
    # future regions should expose these fields through a versioned manifest.
    if manifest["game_build"]["rom_sha256"] in {
        "c431be348a6de9d363ba9f4a358b6b7bd4599ced2bf18562140adc0cda56a3f6",
        "b7b4d0477920b3b053a603e356b1ce243d629576d9db6d8c94aecf2770019505",
    }:
        private = read_memory(handle, address - 176, 56)
        if len(private) == 56:
            result["private_runtime_diagnostic"] = {
                "session_epoch": struct.unpack_from("<I", private, 0)[0],
                "save_update_epoch": struct.unpack_from("<I", private, 32)[0],
                "save_update_generation": struct.unpack_from("<I", private, 36)[0],
                "checkpoint_state": struct.unpack_from("<I", private, 44)[0],
                "cloud_epoch_accepted": bool(private[48]),
                "save_data_update_pending": bool(private[49]),
                "save_data_update_queued": bool(private[50]),
                "flash_save_started": bool(private[51]),
                "recovery_required": bool(private[52]),
            }
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    handle = KERNEL32.OpenProcess(0x0410, False, args.pid)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        addresses = find_bridge(handle, manifest)
        first = [snapshot(handle, address, manifest) for address in addresses]
        time.sleep(0.3)
        second = [snapshot(handle, address, manifest) for address in addresses]
    finally:
        KERNEL32.CloseHandle(handle)
    result = {"pid": args.pid, "bridge_candidates": [
        {"first": left, "second": right} for left, right in zip(first, second)
    ]}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
