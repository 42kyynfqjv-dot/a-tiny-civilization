#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("derive-eltontraits-ecology.py")
SPEC = importlib.util.spec_from_file_location("derive_eltontraits_ecology", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FixedDecimalParsingTest(unittest.TestCase):
    def test_preserves_source_decimal_scale_without_float_conversion(self) -> None:
        self.assertEqual(MODULE.parse_fixed_decimal("6.25", source="fixture"), (625, 2))
        self.assertEqual(MODULE.parse_fixed_decimal("500", source="fixture"), (500, 0))
        self.assertEqual(MODULE.parse_fixed_decimal("0.50", source="fixture"), (50, 2))
        self.assertIsNone(MODULE.parse_fixed_decimal("", source="fixture"))

    def test_rejects_noncanonical_or_excessive_numbers(self) -> None:
        for value in ("-1", "+1", "1e3", ".5", "1.", "1,000", "NaN"):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                MODULE.parse_fixed_decimal(value, source="fixture")
        with self.assertRaises(RuntimeError):
            MODULE.parse_fixed_decimal("0.1234567890", source="fixture")


if __name__ == "__main__":
    unittest.main()
