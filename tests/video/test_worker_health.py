from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class WorkerHealthTests(unittest.TestCase):
    def test_stalled_transform_and_lifecycle_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            source=Path(directory)/'health.cpp'
            binary=Path(directory)/'health'
            source.write_text(r'''#include "worker-health.hpp"
#include <cassert>
using namespace freelane_video;
int main(){
    // Transform emits one picture, then retains two inputs forever.
    WorkerProgress progress(0);progress.observe(0,1,100);
    progress.observe(2,1,200);progress.observe(2,1,500199);
    bool expired=false;try{progress.observe(2,1,500200);}catch(const std::runtime_error&){expired=true;}assert(expired);
    WorkerProgress idle(0);idle.observe(1,1,100);idle.observe(1,2,1000100);idle.observe(1,3,2000100);
    expired=false;try{idle.observe(1,3,5000100);}catch(const std::runtime_error&){expired=true;}assert(expired);
    WorkerProgress recovered(0);recovered.observe(2,1,100);recovered.observe(1,2,200);recovered.observe(2,2,500100);recovered.observe(1,3,700000);
    LifecycleProof proof(TargetFailure::identity);
    assert(!proof.accepts(TargetFailure::identity,100)); // Initialization errors cannot pass.
    bool rejected=false;try{proof.mutated(10,0,true,100);}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    rejected=false;try{proof.mutated(10,8,false,100);}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    proof.mutated(10,8,true,100);
    assert(proof.accepts(TargetFailure::identity,200));
    assert(!proof.accepts(TargetFailure::resize,200));
    assert(!proof.accepts(TargetFailure::identity,2000100)); // Generic timeout cannot count.
}''')
            subprocess.run(['c++','-std=c++17','-I',str(ROOT/'scripts/video'),str(source),'-o',str(binary)],check=True,timeout=30)
            subprocess.run([str(binary)],check=True,timeout=5)
