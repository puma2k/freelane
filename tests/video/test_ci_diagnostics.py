from pathlib import Path
import importlib.util
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('ci_status', ROOT/'scripts/ci-test-status.py')
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


class DiagnosticsTests(unittest.TestCase):
    def test_only_allowlisted_broker_source_locations_are_reported(self):
        self.assertEqual(status.broker_panic_lines(
            "thread 'broker_test' (123) panicked at tests/video_broker.rs:261:23:"), [261])
        self.assertEqual(status.broker_panic_lines(
            "thread 'broker_test' panicked at tests/video_broker.rs:255:9:"), [255])
        self.assertEqual(status.broker_panic_lines(
            "thread 'other' panicked at /private/user/data.rs:123:4:"), [])
        self.assertEqual(status.broker_panic_lines(
            "payload tests/video_broker.rs:123:4:"), [])
