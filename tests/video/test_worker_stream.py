from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class WorkerStreamTests(unittest.TestCase):
    def test_persistent_records_setup_and_output_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            src = Path(directory)/'worker.cpp'
            binary = Path(directory)/'worker'
            src.write_text(r'''#include <array>
#include <functional>
#include <limits>
#include <algorithm>
#include "annexb.hpp"
#define max(a,b) windows_max_macro(a,b)
#include "stream-writer.hpp"
#include "worker-setup.hpp"
#include "bgra-nv12.hpp"
#include <cassert>
using namespace freelane_video;
int main(){
    Bytes setup(80,0);setup[0]='F';setup[1]='L';setup[2]='W';setup[3]='1';setup[5]=80;
    setup[15]=1;setup[19]=2;setup[31]=3;setup[39]=4;setup[40]=5;
    setup[63]=6;setup[71]=7;setup[72]=1;setup[73]=64;setup[75]=180;setup[76]=30;
    auto p=worker_setup(setup);assert(p.window==1&&p.pid==2&&p.created==3&&p.incarnation==4);
    assert(p.binding.nonce[0]==5&&p.binding.stream==6&&p.binding.generation==7&&p.width==320&&p.height==180);
    for(size_t at: {6u,20u,77u}){auto bad=setup;bad[at]=1;bool fail=false;try{worker_setup(bad);}catch(const std::runtime_error&){fail=true;}assert(fail);}
    for(size_t at: {15u,19u,31u,39u,40u,63u,71u,76u}){auto bad=setup;bad[at]=0;bool fail=false;try{worker_setup(bad);}catch(const std::runtime_error&){fail=true;}assert(fail);}
    unsigned records=0;Bytes last;
    StreamWriter writer([&](const Bytes& record){++records;last=record;},p.binding,320,180);
    Converted first;first.configuration={1,66,0,30,255,225,0,4,0x67,66,0,30,1,0,2,0x68,0x80};first.access_unit={0,0,0,2,0x65,0x80};first.idr=true;
    writer.write(first,0);
    Converted next;next.access_unit={0,0,0,2,0x41,0x80};
    for(unsigned i=1;i<120;++i)writer.write(next,i*33333ull);
    assert(records==121&&writer.pictures()==120&&last.size()==70);
    assert(last[16]==5&&last[39]==6&&last[47]==7&&last[55]==120&&last[12]==1&&last[13]==64);
    bool fail=false;try{writer.write(next,119*33333ull);}catch(const std::runtime_error&){fail=true;}assert(fail);
    fail=false;try{writer.write(first,120*33333ull);}catch(const std::runtime_error&){fail=true;}assert(fail);
    StreamWriter stalled([](const Bytes&){throw std::runtime_error("stalled");},p.binding,320,180);
    fail=false;try{stalled.write(first,0);}catch(const std::runtime_error&){fail=true;}assert(fail&&stalled.pictures()==0);
    Bytes bgra(16,100);auto small=bgra_nv12(bgra.data(),16,8,2,2,4,2);assert(small.size()==12&&small[0]==102&&small[8]==128);
    fail=false;try{bgra_nv12(bgra.data(),16,8,2,2,3,2);}catch(const std::runtime_error&){fail=true;}assert(fail);
}''')
            subprocess.run(['c++','-std=c++17','-I',str(ROOT/'scripts/video'),str(src),'-o',str(binary)],check=True,timeout=30)
            subprocess.run([str(binary)],check=True,timeout=5)
