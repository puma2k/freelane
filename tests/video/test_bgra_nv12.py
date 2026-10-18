from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class BgraTests(unittest.TestCase):
    def test_strided_real_pixels_and_invalid_surfaces(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'convert.cpp'
            binary = Path(directory) / 'convert'
            source.write_text(r'''#include "bgra-nv12.hpp"
#include <cassert>
using namespace freelane_video;
int main() {
    // Two-by-two source, deliberately padded rows and ignored alpha.
    Bytes pixels(24,255);
    for (size_t row: {0u,12u}) for (size_t x: {0u,4u}) {
        pixels[row+x]=pixels[row+x+1]=pixels[row+x+2]=100;
    }
    auto gray=bgra_nv12(pixels.data(),pixels.size(),12,2,2);
    assert(gray.size()==320*180*3/2);
    for(size_t i=0;i<320*180;++i) assert(gray[i]==102);
    for(size_t i=320*180;i<gray.size();++i) assert(gray[i]==128);
    // Pure red gives Y=82, U=90, V=240, including the final chroma pair.
    for (size_t row: {0u,12u}) for (size_t x: {0u,4u}) {
        pixels[row+x]=pixels[row+x+1]=0; pixels[row+x+2]=255;
    }
    auto red=bgra_nv12(pixels.data(),pixels.size(),12,2,2);
    assert(red[0]==82 && red[320*180-1]==82);
    assert(red[320*180]==90 && red[320*180+1]==240);
    assert(red[red.size()-2]==90 && red.back()==240);
    unsigned failures=0;
    auto reject=[&](const uint8_t* data,size_t bytes,size_t stride,unsigned w,unsigned h){
        try {bgra_nv12(data,bytes,stride,w,h);} catch(const std::runtime_error&){++failures;}
    };
    reject(nullptr,24,12,2,2); reject(pixels.data(),19,12,2,2);
    reject(pixels.data(),24,7,2,2); reject(pixels.data(),24,12,0,2);
    reject(pixels.data(),24,12,1921,2); reject(pixels.data(),24,12,2,1081);
    reject(pixels.data(),24,SIZE_MAX,2,2);
    assert(failures==7);
}
''')
            subprocess.run(['c++', '-std=c++17', '-I', str(ROOT/'scripts/video'),
                            str(source), '-o', str(binary)], check=True, timeout=30)
            subprocess.run([str(binary)], check=True, timeout=5)
