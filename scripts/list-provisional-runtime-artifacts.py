#!/usr/bin/env python3
"""List the complete relative runtime closure of one provisional composition."""

import argparse
import json
import pathlib


TILE_INDEX_MEDIA_TYPE = "application/vnd.atinycivilization.tile-index+json"
DE441_RUNTIME_INPUTS = (
    "data/source-cache/jpl-de441/de441_part-1.bsp",
    "data/source-cache/jpl-de441/de441_part-2.bsp",
)


def safe_relative(value: str) -> pathlib.PurePosixPath:
    path = pathlib.PurePosixPath(value)
    if path.is_absolute() or not path.parts or any(part in ("", ".", "..") for part in path.parts):
        raise RuntimeError(f"unsafe staged artifact path: {value}")
    return path


def load_json(path: pathlib.PurePosixPath) -> dict:
    return json.loads(pathlib.Path(path).read_bytes())


def closure(composition_name: str) -> list[str]:
    composition_path = safe_relative(composition_name)
    composition = load_json(composition_path)
    paths = {str(composition_path)}
    traversed_roots: set[str] = set()
    for item in composition["earth_layers"] + composition["world_components"]:
        release = item["release"]
        root_path = safe_relative(release["artifact_path"])
        paths.add(str(root_path))
        if release["media_type"] != TILE_INDEX_MEDIA_TYPE or str(root_path) in traversed_roots:
            continue
        traversed_roots.add(str(root_path))
        root = load_json(root_path)
        suffix = pathlib.PurePosixPath("layers") / root["layer_id"] / "root.index"
        if root_path.parts[-len(suffix.parts) :] != suffix.parts:
            raise RuntimeError(f"tile-tree root has an unexpected namespace: {root_path}")
        prefix = pathlib.PurePosixPath(*root_path.parts[: -len(suffix.parts)])
        pending = list(root["entries"])
        while pending:
            entry = pending.pop()
            relative = safe_relative(str(prefix / safe_relative(entry["artifact"]["path"])))
            text = str(relative)
            if text in paths:
                raise RuntimeError(f"duplicate or cyclic staged artifact path: {text}")
            paths.add(text)
            if entry["kind"] == "index":
                pending.extend(load_json(relative)["entries"])
    paths.update(DE441_RUNTIME_INPUTS)
    return sorted(paths)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("composition")
    args = parser.parse_args()
    for path in closure(args.composition):
        print(path)


if __name__ == "__main__":
    main()
