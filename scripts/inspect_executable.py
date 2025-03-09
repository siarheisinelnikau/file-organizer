"""Inspect a packaged PE without executing it. Uses only Python's standard library."""

import argparse
import hashlib
import json
import struct
from pathlib import Path


def inspect(path: Path) -> dict:
    data = path.read_bytes()

    def u16(offset: int) -> int:
        return struct.unpack_from("<H", data, offset)[0]

    def u32(offset: int) -> int:
        return struct.unpack_from("<I", data, offset)[0]

    if data[:2] != b"MZ":
        raise ValueError("Missing DOS executable header")
    pe = u32(60)
    if data[pe : pe + 4] != b"PE\0\0" or u16(pe + 4) != 0x8664:
        raise ValueError("Expected a Windows x64 PE executable")
    optional = pe + 24
    if u16(optional) != 0x20B or u16(optional + 68) != 2:
        raise ValueError("Expected PE32+ with the Windows GUI subsystem")
    section_table = optional + u16(pe + 20)
    sections = []
    for index in range(u16(pe + 6)):
        offset = section_table + index * 40
        sections.append((u32(offset + 12), max(u32(offset + 8), u32(offset + 16)), u32(offset + 20)))

    def file_offset(rva: int) -> int:
        for address, size, raw in sections:
            if address <= rva < address + size:
                result = raw + rva - address
                if result >= len(data):
                    break
                return result
        raise ValueError(f"Unmapped PE address: {rva:#x}")

    imports = []
    import_rva = u32(optional + 120)
    if import_rva:
        descriptor = file_offset(import_rva)
        for _ in range(1024):
            name_rva = u32(descriptor + 12)
            if not name_rva:
                break
            offset = file_offset(name_rva)
            end = data.index(b"\0", offset, min(offset + 256, len(data)))
            imports.append(data[offset:end].decode("ascii"))
            descriptor += 20
        else:
            raise ValueError("Unterminated PE import table")
    if any(name.lower().startswith(("vcruntime", "msvcp")) for name in imports):
        raise ValueError("Executable depends on an external MSVC runtime DLL")
    return {"path": str(path.resolve()), "bytes": len(data), "machine": "AMD64", "subsystem": "Windows GUI",
            "sha256": hashlib.sha256(data).hexdigest(), "imports": sorted(set(imports), key=str.lower)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    arguments = parser.parse_args()
    print(json.dumps(inspect(arguments.path), indent=2))
