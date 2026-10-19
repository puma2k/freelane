#pragma once
#include "annexb.hpp"
namespace freelane_video {
enum class TargetFailure {unavailable,identity,resize};
struct TargetError:std::runtime_error {
    TargetFailure reason;
    explicit TargetError(TargetFailure r):std::runtime_error("Capture target changed"),reason(r){}
};
inline void target_check(bool valid,TargetFailure reason){if(!valid)throw TargetError(reason);}
class WorkerProgress {
    uint64_t last_output_,pictures_=0,pressure_since_=0;bool pressured_=false;
public:
    explicit WorkerProgress(uint64_t now):last_output_(now){}
    void observe(uint64_t pending,uint64_t pictures,uint64_t now){
        require(pending<=2&&pictures>=pictures_,"Invalid encoder progress");
        if(pictures>pictures_){pictures_=pictures;last_output_=now;}
        if(pending==2){if(!pressured_){pressured_=true;pressure_since_=now;}
            require(now-pressure_since_<500000,"Encoder pressure deadline exceeded");
        }else pressured_=false;
        require(now-last_output_<3000000,"Encoded output heartbeat stalled");
    }
};
class LifecycleProof {
    TargetFailure expected_;bool applied_=false;uint64_t at_=0;
public:
    explicit LifecycleProof(TargetFailure expected):expected_(expected){}
    void mutated(uint64_t submitted,uint64_t pictures,bool succeeded,uint64_t now){
        require(submitted>=10&&pictures>0&&succeeded,"Lifecycle mutation was not exercised");
        applied_=true;at_=now;
    }
    bool accepts(TargetFailure reason,uint64_t now)const{return applied_&&reason==expected_&&now>=at_&&now-at_<2000000;}
};
}
