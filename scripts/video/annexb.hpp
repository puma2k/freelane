#pragma once
// Experimental generated-fixture adapter; no capture authorization lives here.
#include <cstdint>
#include <vector>
#include <stdexcept>
#include <ostream>
namespace freelane_video {
using Bytes=std::vector<uint8_t>;
inline void require(bool ok,const char* message){if(!ok)throw std::runtime_error(message);}
struct Converted {Bytes configuration,access_unit;bool idr=false;};
class PacketConverter {
    Bytes sps_,pps_;bool emitted_=false;
    static size_t prefix(const Bytes& b,size_t i){if(i+3<=b.size()&&b[i]==0&&b[i+1]==0){if(b[i+2]==1)return 3;if(i+4<=b.size()&&b[i+2]==0&&b[i+3]==1)return 4;}return 0;}
    static void parameter(Bytes& saved,const Bytes& fresh){require(saved.empty()||saved==fresh,"H264 parameter change requires generation restart");saved=fresh;}
    static void u16(Bytes& out,size_t n){require(n<=65535,"H264 parameter too large");out.push_back(static_cast<uint8_t>(n>>8));out.push_back(static_cast<uint8_t>(n));}
public:
    Converted convert(const Bytes& bytes){
        require(!bytes.empty()&&bytes.size()<=2*1024*1024,"Annex B packet exceeds bounds");
        Bytes sps=sps_,pps=pps_;Converted out;bool dependent=false;size_t position=0,count=0;
        while(position<bytes.size()){
            size_t code=prefix(bytes,position);require(code!=0,"Expected Annex B start code");size_t begin=position+code,next=begin;
            while(next<bytes.size()&&!prefix(bytes,next))++next;
            size_t end=next;while(end>begin&&bytes[end-1]==0)--end;
            require(end>begin&&++count<=256,"Empty/too many Annex B NALs");Bytes nal(bytes.begin()+begin,bytes.begin()+end);
            require((nal[0]&128)==0,"Forbidden H264 bit");unsigned type=nal[0]&31;
            if(type==7){require(nal.size()>=4&&nal[1]==66,"Expected baseline SPS");parameter(sps,nal);}
            else if(type==8){parameter(pps,nal);}
            else{
                require(type==1||type==5||type==6||type==9||type==12,"Unsupported H264 NAL");
                if(type==5)out.idr=true;if(type==1)dependent=true;
                for(int shift=24;shift>=0;shift-=8)out.access_unit.push_back(static_cast<uint8_t>(nal.size()>>shift));
                out.access_unit.insert(out.access_unit.end(),nal.begin(),nal.end());
            }
            position=next;
        }
        require(!sps.empty()&&!pps.empty(),"H264 parameter sets missing before pictures");require(!(out.idr&&dependent),"Mixed IDR/dependent slices");
        require(out.access_unit.empty()||out.idr||dependent,"Access unit contains no picture");
        require(out.access_unit.size()<=2*1024*1024,"AVCC access unit exceeds bounds");
        if(!emitted_){out.configuration={1,66,sps[2],sps[3],255,225};u16(out.configuration,sps.size());out.configuration.insert(out.configuration.end(),sps.begin(),sps.end());out.configuration.push_back(1);u16(out.configuration,pps.size());out.configuration.insert(out.configuration.end(),pps.begin(),pps.end());require(out.configuration.size()<=65536,"Configuration exceeds bounds");}
        sps_=std::move(sps);pps_=std::move(pps);emitted_=true;return out;
    }
};
class PacketWriter {
    std::ostream& output_;size_t written_=0;uint64_t last_pts_=0;bool started_=false;
    void record(uint8_t kind,bool idr,const Bytes& payload,uint64_t sequence,uint64_t pts){
        require(!payload.empty()&&payload.size()<=(kind==1?65536:2*1024*1024),"Invalid packet payload size");
        require(written_+64+payload.size()<=4*1024*1024,"Raw fixture exceeds bounds");
        uint8_t h[64]={'F','L','V','1',kind,static_cast<uint8_t>(idr),66,0};
        auto integer=[&](size_t at,uint64_t n,unsigned width){for(unsigned i=0;i<width;++i)h[at+i]=static_cast<uint8_t>(n>>((width-i-1)*8));};
        integer(8,payload.size(),4);integer(12,320,2);integer(14,180,2);for(size_t i=16;i<32;++i)h[i]=7;
        integer(32,1,8);integer(40,1,8);integer(48,sequence,8);integer(56,pts,8);
        output_.write(reinterpret_cast<const char*>(h),64);output_.write(reinterpret_cast<const char*>(payload.data()),payload.size());require(static_cast<bool>(output_),"Cannot write raw fixture");written_+=64+payload.size();
    }
public:
    unsigned pictures=0,configurations=0,idrs=0;
    explicit PacketWriter(std::ostream& output):output_(output){}
    void write(const Converted& packet,uint64_t pts){
        if(!packet.configuration.empty()){require(configurations==0,"Repeated raw configuration");record(1,false,packet.configuration,0,0);++configurations;}
        if(packet.access_unit.empty())return;
        require(configurations==1&&pictures<30,"Raw picture/configuration count invalid");
        require(!started_?packet.idr:pts>last_pts_,"Raw first picture/time invalid");
        record(2,packet.idr,packet.access_unit,pictures+1,pts);++pictures;if(packet.idr)++idrs;started_=true;last_pts_=pts;
    }
};
}
