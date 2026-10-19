#pragma once
#include "stream-writer.hpp"
namespace freelane_video {
struct WorkerSetup {
    uint64_t window=0,created=0,incarnation=0;uint32_t pid=0;
    StreamBinding binding;unsigned width=0,height=0;
};
inline WorkerSetup worker_setup(const Bytes& b){
    require(b.size()==80&&b[0]=='F'&&b[1]=='L'&&b[2]=='W'&&b[3]=='1'&&b[4]==0&&b[5]==80,"Invalid worker setup");
    for(size_t i:{6u,7u,20u,21u,22u,23u,77u,78u,79u})require(b[i]==0,"Reserved worker setup field");
    auto integer=[&](size_t at,unsigned n){uint64_t result=0;for(unsigned i=0;i<n;++i)result=(result<<8)|b[at+i];return result;};
    WorkerSetup p;p.window=integer(8,8);p.pid=static_cast<uint32_t>(integer(16,4));p.created=integer(24,8);p.incarnation=integer(32,8);
    std::copy(b.begin()+40,b.begin()+56,p.binding.nonce.begin());p.binding.stream=integer(56,8);p.binding.generation=integer(64,8);
    p.width=static_cast<unsigned>(integer(72,2));p.height=static_cast<unsigned>(integer(74,2));
    require(p.window&&p.pid&&p.created&&p.incarnation&&b[76]==30,"Incomplete worker identity/cadence");
    valid_binding(p.binding);valid_dimensions(p.width,p.height);return p;
}
}
