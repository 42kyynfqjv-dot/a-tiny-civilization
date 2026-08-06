#!/usr/bin/env python3
"""Verify the exact, credential-free ERA5 normal-period request contract."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path


def load_acquisition_module():
    path = Path(__file__).with_name("acquire-era5-monthly-climate.py")
    spec = importlib.util.spec_from_file_location("era5_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load ERA5 acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical_digest(value: object) -> str:
    material = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(material).hexdigest()


def main() -> None:
    era5 = load_acquisition_module()
    years = list(range(era5.NORMAL_START_YEAR, era5.NORMAL_END_YEAR + 1))
    requests = [era5.request_for(year) for year in years]
    assert years == list(range(1981, 2011))
    assert era5.DATASET == "reanalysis-era5-single-levels-monthly-means"
    assert era5.PRODUCT_TYPE == "monthly_averaged_reanalysis"
    assert era5.MONTHS == tuple(f"{month:02d}" for month in range(1, 13))
    assert era5.VARIABLES == (
        "2m_temperature",
        "total_precipitation",
        "10m_u_component_of_wind",
        "10m_v_component_of_wind",
        "sea_surface_temperature",
        "sea_ice_cover",
    )
    assert all(request["year"] == [str(year)] for request, year in zip(requests, years))
    assert all(request["data_format"] == "netcdf" for request in requests)
    assert era5.output_path(Path("/tmp"), 1981).name.endswith("1981.zip")
    assert era5.legacy_output_path(Path("/tmp"), 1981).name.endswith("1981.nc")
    assert era5.EXPECTED_ARCHIVE_MEMBERS == (
        "data_stream-moda_stepType-avgua.nc",
        "data_stream-moda_stepType-avgad.nc",
    )
    assert canonical_digest({"dataset": era5.DATASET, "requests": requests}) == (
        "546a6f02091abf2ccd320523abdeefedc7e40c924ab7298672c22ef141241a6a"
    )
    print("ERA5 normal-period request contract is stable.")


if __name__ == "__main__":
    main()
