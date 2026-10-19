from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ClientCropTests(unittest.TestCase):
    def test_client_pixels_exclude_chrome_and_reject_invalid_rectangles(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'crop.cpp'
            binary = Path(directory) / 'crop'
            source.write_text(r'''#include "bgra-nv12.hpp"
#include <cassert>
using namespace freelane_video;
int main() {
    // Red chrome around a gray odd-sized client; deliberately padded rows.
    Bytes pixels(32*6,0);
    for (unsigned y=0;y<6;++y) for(unsigned x=0;x<6;++x) {
        auto p=pixels.data()+y*32+x*4;p[2]=255;
        if(x>=1&&x<4&&y>=2&&y<5)p[0]=p[1]=p[2]=100;
    }
    auto result=bgra_nv12_crop(pixels.data(),pixels.size(),32,6,6,1,2,3,3,2,2);
    assert(result.size()==6);
    for(unsigned i=0;i<4;++i)assert(result[i]==102);
    assert(result[4]==128&&result[5]==128);
    unsigned failures=0;
    auto reject=[&](size_t bytes,unsigned x,unsigned y,unsigned w,unsigned h){
        try{bgra_nv12_crop(pixels.data(),bytes,32,6,6,x,y,w,h,2,2);}
        catch(const std::runtime_error&){++failures;}
    };
    reject(pixels.size(),6,0,1,1);reject(pixels.size(),0,6,1,1);
    reject(pixels.size(),1,2,0,3);reject(pixels.size(),1,2,3,0);
    reject(pixels.size(),4,2,3,3);reject(pixels.size(),1,4,3,3);
    reject(pixels.size(),~0u,2,3,3);reject(100,1,2,3,3);
    assert(failures==8);
}
''')
            subprocess.run(['c++', '-std=c++17', '-I', str(ROOT/'scripts/video'),
                            str(source), '-o', str(binary)], check=True, timeout=30)
            subprocess.run([str(binary)], check=True, timeout=5)
