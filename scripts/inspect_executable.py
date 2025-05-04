"""Inspect a packaged PE without executing it. Uses only Python's standard library."""

import argparse
import hashlib
import json
import struct
from pathlib import Path


def inspect(path: Path, expected_version: str | None = None, icon_path: Path | None = None) -> dict:
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
    resources = {}
    resource_rva = u32(optional + 128)
    if resource_rva:
        base = file_offset(resource_rva)

        def walk(relative: int, keys: tuple = ()):
            if len(keys) > 3:
                raise ValueError("Unexpected resource tree depth")
            directory = base + relative
            count = u16(directory + 12) + u16(directory + 14)
            if count > 1024:
                raise ValueError("Resource directory is too large")
            for index in range(count):
                entry = directory + 16 + index * 8
                name, target = u32(entry), u32(entry + 4)
                if name & 0x80000000:
                    text = base + (name & 0x7FFFFFFF)
                    name = data[text + 2:text + 2 + u16(text) * 2].decode("utf-16le")
                if target & 0x80000000:
                    walk(target & 0x7FFFFFFF, (*keys, name))
                else:
                    entry_data = base + target
                    start, size = file_offset(u32(entry_data)), u32(entry_data + 4)
                    if start + size > len(data):
                        raise ValueError("Truncated resource")
                    resources[(*keys, name)] = data[start:start + size]

        walk(0)

    icon_sizes = []
    for (kind, _, language), group in resources.items():
        if kind != 14:
            continue
        reserved, image_type, count = struct.unpack_from("<HHH", group)
        if (reserved, image_type) != (0, 1) or len(group) != 6 + 14 * count:
            raise ValueError("Invalid icon group")
        expected = {}
        if icon_path:
            ico = icon_path.read_bytes()
            if struct.unpack_from("<HHH", ico) != (0, 1, count):
                raise ValueError("Embedded/source icon counts differ")
            for index in range(count):
                width, height, _, _, _, bits, size, offset = struct.unpack_from("<BBBBHHII", ico, 6 + 16 * index)
                expected[(width or 256, height or 256, bits)] = ico[offset:offset + size]
        for index in range(count):
            width, height, _, _, _, bits, size, identifier = struct.unpack_from("<BBBBHHIH", group, 6 + 14 * index)
            width, height = width or 256, height or 256
            payload = resources.get((3, identifier, language))
            if payload is None or len(payload) != size:
                raise ValueError("An embedded icon frame is missing or truncated")
            if icon_path and payload != expected.get((width, height, bits)):
                raise ValueError("Embedded icon differs from the checked-in artwork")
            icon_sizes.append([width, height])

    versions = {}
    for (kind, _, _), value in resources.items():
        if kind != 16:
            continue
        key_end = 6
        while value[key_end:key_end + 2] != b"\0\0":
            key_end += 2
            if key_end >= len(value):
                raise ValueError("Invalid version resource")
        key = value[6:key_end].decode("utf-16le")
        fixed = (key_end + 2 + 3) & ~3
        if key != "VS_VERSION_INFO" or struct.unpack_from("<I", value, fixed)[0] != 0xFEEF04BD:
            raise ValueError("Invalid Windows version header")
        for name, offset in [("file_version", 8), ("product_version", 16)]:
            major_minor, patch_build = struct.unpack_from("<II", value, fixed + offset)
            versions[name] = f"{major_minor >> 16}.{major_minor & 65535}.{patch_build >> 16}.{patch_build & 65535}"
    if expected_version and (versions.get("file_version") != expected_version + ".0" or versions.get("product_version") != expected_version + ".0"):
        raise ValueError("Windows file/product version does not match Cargo.toml")
    if icon_path and sorted(icon_sizes) != [[s, s] for s in [16, 20, 24, 32, 40, 48, 64, 128, 256]]:
        raise ValueError("Expected the full DPI-aware icon size set")
    return {"path": str(path.resolve()), "bytes": len(data), "machine": "AMD64", "subsystem": "Windows GUI",
            "sha256": hashlib.sha256(data).hexdigest(), "imports": sorted(set(imports), key=str.lower),
            "icon_sizes": sorted(icon_sizes), **versions}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    parser.add_argument("--expect-version")
    parser.add_argument("--icon", type=Path)
    arguments = parser.parse_args()
    print(json.dumps(inspect(arguments.path, arguments.expect_version, arguments.icon), indent=2))
