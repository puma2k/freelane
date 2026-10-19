#pragma once
#include "annexb.hpp"
#include <array>
#include <functional>
#include <limits>
namespace freelane_video {
struct StreamBinding {std::array<uint8_t,16> nonce{};uint64_t stream=0,generation=0;};
inline void valid_binding(const StreamBinding& b){
    bool nonzero=false;for(auto n:b.nonce)nonzero=nonzero||n!=0;
    require(nonzero&&b.stream&&b.generation,"Invalid stream binding");
}
inline void valid_dimensions(unsigned w,unsigned h){require(w>0&&h>0&&w<=1920&&h<=1080&&w%2==0&&h%2==0,"Invalid coded dimensions");}
class StreamWriter {
    std::function<void(const Bytes&)> output_;
    StreamBinding binding_;unsigned width_,height_;
    uint64_t pictures_=0,last_pts_=0;bool configured_=false,healthy_=true;
    void record(uint8_t kind,bool idr,const Bytes& payload,uint64_t sequence,uint64_t pts){
        require(!payload.empty()&&payload.size()<=(kind==1?65536:2*1024*1024),"Invalid streaming payload");
        Bytes bytes(64,0);bytes[0]='F';bytes[1]='L';bytes[2]='V';bytes[3]='1';bytes[4]=kind;bytes[5]=idr;bytes[6]=66;
        auto integer=[&](size_t at,uint64_t n,unsigned count){for(unsigned i=0;i<count;++i)bytes[at+i]=static_cast<uint8_t>(n>>((count-i-1)*8));};
        integer(8,payload.size(),4);integer(12,width_,2);integer(14,height_,2);
        std::copy(binding_.nonce.begin(),binding_.nonce.end(),bytes.begin()+16);
        integer(32,binding_.stream,8);integer(40,binding_.generation,8);integer(48,sequence,8);integer(56,pts,8);
        bytes.insert(bytes.end(),payload.begin(),payload.end());output_(bytes);
    }
public:
    StreamWriter(std::function<void(const Bytes&)> output,StreamBinding binding,unsigned w,unsigned h)
        :output_(std::move(output)),binding_(binding),width_(w),height_(h){valid_binding(binding);valid_dimensions(w,h);require(bool(output_),"Missing stream sink");}
    uint64_t pictures()const{return pictures_;}
    void write(const Converted& packet,uint64_t pts){
        require(healthy_,"Stream output already failed");
        try {
            if(!packet.configuration.empty()){
                require(!configured_,"Codec change requires a fresh generation");
                record(1,false,packet.configuration,0,0);configured_=true;
            }
            if(packet.access_unit.empty())return;
            require(configured_&&pictures_<std::numeric_limits<uint64_t>::max(),"Invalid stream sequence/configuration");
            require(pictures_?pts>last_pts_:packet.idr,"Stream must start with IDR and advance timestamps");
            record(2,packet.idr,packet.access_unit,pictures_+1,pts);++pictures_;last_pts_=pts;
        }catch(...){healthy_=false;throw;}
    }
};
}
