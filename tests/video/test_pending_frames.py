"""Execute the same portable C++ occupancy accounting used by the native probe."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]

class PendingFramesTests(unittest.TestCase):
    def test_submission_peak_survives_subsequent_drain(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'pending.cpp'
            binary = Path(directory) / 'pending'
            source.write_text('''#include "pending-frames.hpp"
int main() {
    unsigned peak = pending_peak(0, 2, 0);
    peak = pending_peak(peak, 3, 0); // Third submission before draining.
    peak = pending_peak(peak, 3, 1); // Drain must not hide the peak.
    if (peak != 3) return 1;
    if (pending_peak(0, 1, 0) != 1) return 2;
    try { pending_peak(0, 1, 2); return 3; }
    catch (const std::invalid_argument&) {}
    return 0;
}
''')
            subprocess.run(['c++', '-std=c++17', '-I', str(ROOT/'scripts/video'), str(source), '-o', str(binary)], check=True, timeout=30)
            subprocess.run([str(binary)], check=True, timeout=5)
