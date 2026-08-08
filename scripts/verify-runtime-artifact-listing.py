#!/usr/bin/env python3
"""Exercise recursive runtime-path discovery without the multi-gigabyte data tree."""

import importlib.util
import json
import os
import pathlib
import tempfile


project_root = pathlib.Path(__file__).resolve().parents[1]
module_path = project_root / "scripts" / "list-provisional-runtime-artifacts.py"
spec = importlib.util.spec_from_file_location("runtime_artifacts", module_path)
if spec is None or spec.loader is None:
    raise RuntimeError("could not load runtime artifact listing helper")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    previous = pathlib.Path.cwd()
    try:
        os.chdir(root)
        index_path = pathlib.Path("release/layers/elevation/root.index")
        leaf_path = pathlib.Path("release/layers/elevation/l6/0001000000000000.tile")
        index_path.parent.mkdir(parents=True)
        leaf_path.parent.mkdir(parents=True)
        leaf_path.write_bytes(b"leaf")
        index_path.write_text(
            json.dumps(
                {
                    "index_schema_version": 1,
                    "layer_id": "elevation",
                    "entries": [
                        {
                            "kind": "tile",
                            "s2_cell_id": "0001000000000000",
                            "s2_level": 6,
                            "artifact": {
                                "path": "layers/elevation/l6/0001000000000000.tile"
                            },
                        }
                    ],
                },
                separators=(",", ":"),
            )
        )
        component_path = pathlib.Path("release/component.bin")
        component_path.write_bytes(b"component")
        composition_path = pathlib.Path("composition.json")
        shared_release = {
            "artifact_path": str(index_path),
            "media_type": module.TILE_INDEX_MEDIA_TYPE,
        }
        composition_path.write_text(
            json.dumps(
                {
                    "earth_layers": [{"release": shared_release}, {"release": shared_release}],
                    "world_components": [
                        {
                            "release": {
                                "artifact_path": str(component_path),
                                "media_type": "application/octet-stream",
                            }
                        }
                    ],
                }
            )
        )
        observed = module.closure(str(composition_path))
        expected = sorted(
            {
                str(composition_path),
                str(index_path),
                str(leaf_path),
                str(component_path),
                *module.DE441_RUNTIME_INPUTS,
            }
        )
        if observed != expected:
            raise RuntimeError(f"runtime closure mismatch: {observed!r}")
    finally:
        os.chdir(previous)

print("Recursive provisional runtime artifact discovery is stable.")
