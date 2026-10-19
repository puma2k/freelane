#pragma once
// CPU reference conversion for the owned capture probe, not a zero-copy path.
#include "annexb.hpp"
#include <algorithm>
namespace freelane_video {
inline Bytes bgra_nv12(const uint8_t* data,size_t bytes,size_t stride,unsigned width,unsigned height,unsigned w=320,unsigned h=180) {
    require(data&&width>0&&height>0&&width<=1920&&height<=1080,"Invalid BGRA surface");
    require(stride>=size_t(width)*4&&stride<=1920*4+65536,"Invalid BGRA stride");
    require(bytes>=stride*(height-1)+size_t(width)*4,"Truncated BGRA surface");
    require(w>0&&h>0&&w<=1920&&h<=1080&&w%2==0&&h%2==0,"Invalid NV12 dimensions");
    Bytes output(w*h*3/2);
    auto pixel=[&](unsigned x,unsigned y){return data+(size_t(y)*height/h)*stride+(size_t(x)*width/w)*4;};
    auto clipped=[](int n){return static_cast<uint8_t>((std::clamp)(n,0,255));};
    for(unsigned y=0;y<h;++y) for(unsigned x=0;x<w;++x){
        auto p=pixel(x,y);output[y*w+x]=clipped(((66*p[2]+129*p[1]+25*p[0]+128)>>8)+16);
    }
    for(unsigned y=0;y<h;y+=2) for(unsigned x=0;x<w;x+=2){
        int b=0,g=0,r=0;
        for(unsigned dy=0;dy<2;++dy) for(unsigned dx=0;dx<2;++dx){auto p=pixel(x+dx,y+dy);b+=p[0];g+=p[1];r+=p[2];}
        b=(b+2)/4;g=(g+2)/4;r=(r+2)/4;
        // Division avoids implementation-defined negative right shift in C++17.
        auto rounded=[](int n){n+=128;return n>=0?n/256:-((-n+255)/256);};
        size_t at=w*h+(y/2)*w+x;
        output[at]=clipped(rounded(-38*r-74*g+112*b)+128);
        output[at+1]=clipped(rounded(112*r-94*g-18*b)+128);
    }
    return output;
}
inline Bytes bgra_nv12_crop(const uint8_t* data,size_t bytes,size_t stride,unsigned width,unsigned height,
                           unsigned left,unsigned top,unsigned client_width,unsigned client_height,unsigned w,unsigned h) {
    require(data&&width>0&&height>0&&width<=1920&&height<=1080,"Invalid BGRA surface");
    require(stride>=size_t(width)*4&&stride<=1920*4+65536,"Invalid BGRA stride");
    require(bytes>=stride*(height-1)+size_t(width)*4,"Truncated BGRA surface");
    require(left<width&&top<height&&client_width>0&&client_height>0&&
            client_width<=width-left&&client_height<=height-top,"Invalid client crop");
    const size_t offset=size_t(top)*stride+size_t(left)*4;
    return bgra_nv12(data+offset,bytes-offset,stride,client_width,client_height,w,h);
}

}
