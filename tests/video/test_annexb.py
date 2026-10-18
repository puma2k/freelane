from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
class AnnexBTests(unittest.TestCase):
    def test_packet_conversion_and_bounded_wire_output(self):
        with tempfile.TemporaryDirectory() as directory:
            source=Path(directory)/'packets.cpp';binary=Path(directory)/'packets'
            source.write_text(r'''#include "annexb.hpp"
#include <sstream>
#include <cassert>
using namespace freelane_video;
int main() {
    const std::vector<uint8_t> first={0,0,0,1,0x67,66,0,30,0,0,1,0x68,0x80,0,0,0,1,0x65,0x80};
    PacketConverter converter;auto packet=converter.convert(first);
    assert(packet.idr && packet.configuration==std::vector<uint8_t>({1,66,0,30,255,225,0,4,0x67,66,0,30,1,0,2,0x68,0x80}));
    assert(packet.access_unit==std::vector<uint8_t>({0,0,0,2,0x65,0x80}));
    auto repeated=converter.convert(first);assert(repeated.configuration.empty() && repeated.idr);
    auto dependent=converter.convert({0,0,1,0x41,0x80});assert(!dependent.idr && dependent.configuration.empty());
    std::ostringstream output;PacketWriter writer(output);writer.write(packet,0);writer.write(dependent,33333);
    auto wire=output.str();assert(wire.substr(0,4)=="FLV1" && uint8_t(wire[4])==1 && uint8_t(wire[6])==66 && uint8_t(wire[11])==17);
    assert(wire.substr(81,4)=="FLV1" && uint8_t(wire[86])==1 && writer.pictures==2 && writer.configurations==1 && writer.idrs==1);
    for(auto bad:std::vector<std::vector<uint8_t>>{{},{0,0,1},{1,0,0,1,0x65},{0,0,1,0xe5,0x80},{0,0,1,0x61,0x80,0,0,1,0x65,0x80},{0,0,1,0x42,0x80}}) {
        bool rejected=false;try {converter.convert(bad);}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    }
    auto changed=first;changed[7]=31;bool rejected=false;try{converter.convert(changed);}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    rejected=false;try {PacketConverter fresh;fresh.convert({0,0,1,0x65,0x80});}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    rejected=false;try {converter.convert(std::vector<uint8_t>(2*1024*1024+1));}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    rejected=false;try {writer.write(packet,66666);}catch(const std::runtime_error&){rejected=true;}assert(rejected);
    return 0;
}
''')
            subprocess.run(['c++','-std=c++17','-I',str(ROOT/'scripts/video'),str(source),'-o',str(binary)],check=True,timeout=30)
            subprocess.run([str(binary)],check=True,timeout=5)
