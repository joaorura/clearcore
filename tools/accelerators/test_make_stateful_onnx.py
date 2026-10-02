#!/usr/bin/env python3
"""Automated unit and integration test for make_stateful_onnx graph surgery."""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from make_stateful_onnx import (
    locate_stock_models,
    perform_surgery,
    verify_stateful_models,
)


class TestStatefulOnnx(unittest.TestCase):
    def test_stateful_onnx_parity_s8_vs_s1(self):
        """Verify that stateful graphs exhibit zero numerical divergence across 8 frames."""
        stock_dir = locate_stock_models()
        with tempfile.TemporaryDirectory() as tmp_out:
            perform_surgery(stock_dir, tmp_out)
            self.assertTrue(os.path.exists(os.path.join(tmp_out, "enc.onnx")))
            self.assertTrue(os.path.exists(os.path.join(tmp_out, "erb_dec.onnx")))
            self.assertTrue(os.path.exists(os.path.join(tmp_out, "df_dec.onnx")))
            self.assertTrue(os.path.exists(os.path.join(tmp_out, "config.ini")))

            success = verify_stateful_models(tmp_out, num_frames=8)
            self.assertTrue(success, "Parity check between S=1 streaming and S=8 batched failed")


if __name__ == "__main__":
    unittest.main()
