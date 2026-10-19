#pragma once
#include "worker-native.hpp"
class WindowCodec {
    ComPtr<IMFTransform> transform_;ComPtr<ICodecAPI> api_;
    DWORD input_=0,output_=0;MFT_OUTPUT_STREAM_INFO info_{};
    freelane_video::PacketConverter converter_;
    freelane_video::StreamWriter writer_;
    unsigned width_,height_;uint64_t submitted_=0;LONGLONG last_pts_=-1;
    std::function<void()> first_picture_;
    void setting(const GUID& key,ULONG value,bool boolean=false){
        VARIANT request;VariantInit(&request);request.vt=boolean?VT_BOOL:VT_UI4;
        if(boolean)request.boolVal=value?VARIANT_TRUE:VARIANT_FALSE;else request.ulVal=value;
        HRESULT set=api_->SetValue(&key,&request);VariantClear(&request);check_hresult(set);
        VARIANT actual;VariantInit(&actual);HRESULT get=api_->GetValue(&key,&actual);
        const bool valid=SUCCEEDED(get)&&(boolean?(actual.vt==VT_BOOL&&actual.boolVal==VARIANT_TRUE):
            ((actual.vt==VT_UI4&&actual.ulVal==value)||(actual.vt==VT_I4&&actual.lVal==static_cast<LONG>(value))));
        VariantClear(&actual);freelane_video::require(valid,"Encoder ignored low-latency configuration");
    }
    ComPtr<IMFMediaType> type(const GUID& subtype){
        ComPtr<IMFMediaType> result;check_hresult(MFCreateMediaType(&result));
        check_hresult(result->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video));check_hresult(result->SetGUID(MF_MT_SUBTYPE,subtype));
        check_hresult(result->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive));
        check_hresult(MFSetAttributeSize(result.Get(),MF_MT_FRAME_SIZE,width_,height_));
        check_hresult(MFSetAttributeRatio(result.Get(),MF_MT_FRAME_RATE,30,1));check_hresult(MFSetAttributeRatio(result.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1));
        if(subtype==MFVideoFormat_H264){check_hresult(result->SetUINT32(MF_MT_AVG_BITRATE,4000000));check_hresult(result->SetUINT32(MF_MT_MPEG2_PROFILE,66));}
        else{check_hresult(result->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES,TRUE));check_hresult(result->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT,TRUE));check_hresult(result->SetUINT32(MF_MT_SAMPLE_SIZE,width_*height_*3/2));check_hresult(result->SetUINT32(MF_MT_DEFAULT_STRIDE,width_));}
        return result;
    }
public:
    unsigned max_pending=0,idrs=0;
    WindowCodec(freelane_video::StreamBinding binding,unsigned width,unsigned height,
        std::function<void(const Bytes&)> output,std::function<void()> first)
        :writer_(std::move(output),binding,width,height),width_(width),height_(height),first_picture_(std::move(first)){
        MFT_REGISTER_TYPE_INFO out{MFMediaType_Video,MFVideoFormat_H264};IMFActivate** entries=nullptr;UINT32 count=0;
        check_hresult(MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,MFT_ENUM_FLAG_SYNCMFT|MFT_ENUM_FLAG_SORTANDFILTER,nullptr,&out,&entries,&count));
        HRESULT activation=MF_E_TOPO_CODEC_NOT_FOUND;
        if(count)activation=entries[0]->ActivateObject(__uuidof(IMFTransform),reinterpret_cast<void**>(transform_.GetAddressOf()));
        for(UINT32 i=0;i<count;++i)entries[i]->Release();CoTaskMemFree(entries);check_hresult(activation);
        HRESULT ids=transform_->GetStreamIDs(1,&input_,1,&output_);if(ids!=E_NOTIMPL)check_hresult(ids);
        check_hresult(transform_.As(&api_));setting(CODECAPI_AVEncNumWorkerThreads,1);
        auto output_type=type(MFVideoFormat_H264),input_type=type(MFVideoFormat_NV12);
        check_hresult(transform_->SetOutputType(output_,output_type.Get(),0));check_hresult(transform_->SetInputType(input_,input_type.Get(),0));
        setting(CODECAPI_AVLowLatencyMode,1,true);setting(CODECAPI_AVEncMPVDefaultBPictureCount,0);
        check_hresult(transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0));check_hresult(transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0));
        check_hresult(transform_->GetOutputStreamInfo(output_,&info_));freelane_video::require(info_.cbSize<=2*1024*1024,"Encoder output buffer exceeds bounds");
    }
    uint64_t pictures()const{return writer_.pictures();}
    uint64_t pending()const{return submitted_-pictures();}
    void keyframe(){
        VARIANT request;VariantInit(&request);request.vt=VT_UI4;request.ulVal=1;
        HRESULT result=api_->SetValue(&CODECAPI_AVEncVideoForceKeyFrame,&request);VariantClear(&request);check_hresult(result);
    }
    void drain(){
        for(unsigned i=0;i<32;++i){
            ComPtr<IMFSample> sample;
            if(!(info_.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES)){
                ComPtr<IMFMediaBuffer> buffer;check_hresult(MFCreateMemoryBuffer((std::max)(info_.cbSize,DWORD(65536)),&buffer));
                check_hresult(MFCreateSample(&sample));check_hresult(sample->AddBuffer(buffer.Get()));
            }
            MFT_OUTPUT_DATA_BUFFER out{};out.dwStreamID=output_;out.pSample=sample.Get();DWORD status=0;
            HRESULT hr=transform_->ProcessOutput(0,1,&out,&status);if(out.pEvents)out.pEvents->Release();
            if(hr==MF_E_TRANSFORM_NEED_MORE_INPUT)return;
            // Own transform-provided samples on every success/error path.
            if(!sample&&out.pSample)sample.Attach(out.pSample);
            check_hresult(hr);freelane_video::require(bool(sample),"Encoder returned no sample");
            DWORD length=0;check_hresult(sample->GetTotalLength(&length));LONGLONG pts=0;check_hresult(sample->GetSampleTime(&pts));
            freelane_video::require(length>0&&length<=2*1024*1024&&pts>=0&&pts>last_pts_&&pictures()<submitted_,"Invalid encoder output sequence/time");
            ComPtr<IMFMediaBuffer> contiguous;check_hresult(sample->ConvertToContiguousBuffer(&contiguous));
            BYTE* data=nullptr;DWORD bytes=0;check_hresult(contiguous->Lock(&data,nullptr,&bytes));Bytes copy;
            try{freelane_video::require(data&&bytes==length,"Invalid encoder buffer");copy.assign(data,data+bytes);}catch(...){contiguous->Unlock();throw;}
            check_hresult(contiguous->Unlock());auto packet=converter_.convert(copy);const auto before=pictures();
            writer_.write(packet,static_cast<uint64_t>(pts/10));last_pts_=pts;
            if(packet.idr)++idrs;if(before==0&&pictures()>0)first_picture_();
        }
        throw std::runtime_error("Unbounded encoder output burst");
    }
    void submit(const Bytes& pixels,uint64_t pts){
        freelane_video::require(pixels.size()==size_t(width_)*height_*3/2&&pending()<2&&pts<=uint64_t(INT64_MAX)/10,"Encoder input exceeds bounds");
        ComPtr<IMFMediaBuffer> buffer;check_hresult(MFCreateMemoryBuffer(static_cast<DWORD>(pixels.size()),&buffer));
        BYTE* data=nullptr;check_hresult(buffer->Lock(&data,nullptr,nullptr));memcpy(data,pixels.data(),pixels.size());check_hresult(buffer->Unlock());check_hresult(buffer->SetCurrentLength(static_cast<DWORD>(pixels.size())));
        ComPtr<IMFSample> sample;check_hresult(MFCreateSample(&sample));check_hresult(sample->AddBuffer(buffer.Get()));
        check_hresult(sample->SetSampleTime(static_cast<LONGLONG>(pts*10)));check_hresult(sample->SetSampleDuration(333333));
        check_hresult(transform_->ProcessInput(input_,sample.Get(),0));++submitted_;max_pending=(std::max)(max_pending,static_cast<unsigned>(pending()));drain();
    }
    void finish(){check_hresult(transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM,input_));check_hresult(transform_->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN,0));drain();check_hresult(transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING,0));}
};
