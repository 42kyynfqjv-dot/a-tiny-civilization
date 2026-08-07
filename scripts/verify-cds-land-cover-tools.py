#!/usr/bin/env python3
"""Verify the land-cover request and no-replacement archive admission boundary."""

from __future__ import annotations

import importlib.util
import tempfile
import zipfile
from pathlib import Path


def acquisition_module():
    path = Path(__file__).with_name("acquire-cds-land-cover.py")
    spec = importlib.util.spec_from_file_location("cds_land_cover_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load land-cover acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_archive(path: Path, members: dict[str, bytes]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in members.items():
            archive.writestr(name, content)


def expect_rejected(module, path: Path) -> None:
    try:
        module.validate_response(path)
    except ValueError:
        return
    raise AssertionError(f"unsafe synthetic response was accepted: {path.name}")


def main() -> int:
    module = acquisition_module()
    assert module.request() == {
        "variable": "all",
        "version": ["v2_1_1"],
        "year": ["2022"],
    }
    with tempfile.TemporaryDirectory(prefix="atc-land-cover-") as directory:
        root = Path(directory)
        valid = root / "valid.zip"
        write_archive(valid, {"global-land-cover-2022.nc": b"CDF\x01evidence"})
        assert module.validate_response(valid) == ("global-land-cover-2022.nc",)

        unsafe = root / "unsafe.zip"
        write_archive(unsafe, {"../escaped.nc": b"not allowed"})
        expect_rejected(module, unsafe)

        wrong_type = root / "wrong-type.zip"
        write_archive(wrong_type, {"readme.txt": b"not NetCDF"})
        expect_rejected(module, wrong_type)

        empty = root / "empty-member.zip"
        write_archive(empty, {"empty.nc": b""})
        expect_rejected(module, empty)

        partial = root / "response.partial"
        partial.write_bytes(b"immutable evidence")
        target = root / "response.zip"
        module.publish_without_replacement(partial, target)
        assert target.read_bytes() == b"immutable evidence"
        conflicting = root / "conflicting.partial"
        conflicting.write_bytes(b"must not replace")
        try:
            module.publish_without_replacement(conflicting, target)
        except FileExistsError:
            pass
        else:
            raise AssertionError("land-cover publication replaced existing evidence")
        assert target.read_bytes() == b"immutable evidence"
    print("CDS land-cover acquisition tools are stable.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
