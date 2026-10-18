#pragma once
// Direct synchronous software-MFT probe: distinguishes packet delivery from file finalization.
#include <mferror.h>
inline ComPtr<IMFMediaType> direct_type(const GUID& subtype) {
    ComPtr<IMFMediaType> type;check_hresult(MFCreateMediaType(&type));
    check_hresult(type->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video));check_hresult(type->SetGUID(MF_MT_SUBTYPE,subtype));
    check_hresult(type->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive));
    check_hresult(MFSetAttributeSize(type.Get(),MF_MT_FRAME_SIZE,320,180));check_hresult(MFSetAttributeRatio(type.Get(),MF_MT_FRAME_RATE,30,1));check_hresult(MFSetAttributeRatio(type.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1));
    if(subtype==MFVideoFormat_H264){check_hresult(type->SetUINT32(MF_MT_AVG_BITRATE,1000000));check_hresult(type->SetUINT32(MF_MT_MPEG2_PROFILE,66));}
    if(subtype==MFVideoFormat_NV12){check_hresult(type->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES,TRUE));check_hresult(type->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT,TRUE));check_hresult(type->SetUINT32(MF_MT_SAMPLE_SIZE,320*180*3/2));check_hresult(type->SetUINT32(MF_MT_DEFAULT_STRIDE,320));}
    return type;
}
inline void direct_encode(Report& r) {
    MFT_REGISTER_TYPE_INFO output_type{MFMediaType_Video,MFVideoFormat_H264};IMFActivate** entries=nullptr;UINT32 count=0;
    check_hresult(MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,MFT_ENUM_FLAG_SYNCMFT|MFT_ENUM_FLAG_SORTANDFILTER,nullptr,&output_type,&entries,&count));
    ComPtr<IMFTransform> transform;HRESULT activation=MF_E_TOPO_CODEC_NOT_FOUND;
    if(count)activation=entries[0]->ActivateObject(__uuidof(IMFTransform),reinterpret_cast<void**>(transform.GetAddressOf()));
    for(UINT32 i=0;i<count;++i)entries[i]->Release();CoTaskMemFree(entries);check_hresult(activation);
    DWORD input_id=0,output_id=0;HRESULT ids=transform->GetStreamIDs(1,&input_id,1,&output_id);if(ids!=E_NOTIMPL)check_hresult(ids);
    auto out=direct_type(MFVideoFormat_H264),in=direct_type(MFVideoFormat_NV12);
    check_hresult(transform->SetOutputType(output_id,out.Get(),0));check_hresult(transform->SetInputType(input_id,in.Get(),0));
    ComPtr<ICodecAPI> api;if(SUCCEEDED(transform.As(&api))){r.direct_low_latency=codec_bool(api.Get(),CODECAPI_AVLowLatencyMode,true);r.direct_zero_b=codec_bool(api.Get(),CODECAPI_AVEncMPVDefaultBPictureCount,false);}
    check_hresult(transform->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0));check_hresult(transform->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0));
    MFT_OUTPUT_STREAM_INFO info{};check_hresult(transform->GetOutputStreamInfo(output_id,&info));
    if(info.cbSize>2*1024*1024)throw hresult_error(E_INVALIDARG);
    const auto start=std::chrono::steady_clock::now();LONGLONG last=-1;unsigned submitted=0;
    auto drain=[&]() {
        for(unsigned attempts=0;attempts<32;++attempts){
            ComPtr<IMFSample> sample;
            if(!(info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES)){
                ComPtr<IMFMediaBuffer> buffer;check_hresult(MFCreateMemoryBuffer((std::max)(info.cbSize,static_cast<DWORD>(65536)),&buffer));
                check_hresult(MFCreateSample(&sample));check_hresult(sample->AddBuffer(buffer.Get()));
            }
            MFT_OUTPUT_DATA_BUFFER data{};data.dwStreamID=output_id;data.pSample=sample.Get();DWORD status=0;
            HRESULT hr=transform->ProcessOutput(0,1,&data,&status);
            if(data.pEvents)data.pEvents->Release();
            if(hr==MF_E_TRANSFORM_NEED_MORE_INPUT)return;
            check_hresult(hr);
            if(!sample&&data.pSample)sample.Attach(data.pSample);
            if(!sample)throw hresult_error(E_UNEXPECTED);
            DWORD length=0;check_hresult(sample->GetTotalLength(&length));
            LONGLONG pts=0;check_hresult(sample->GetSampleTime(&pts));
            if(length==0||length>2*1024*1024||pts<=last||r.direct_packet_count>=30)throw hresult_error(E_INVALIDARG);
            last=pts;
            if(r.direct_packet_count++==0)r.direct_first_packet_us=std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now()-start).count();
        }
        throw hresult_error(E_UNEXPECTED);
    };
    for(unsigned i=0;i<30;++i){
        ComPtr<IMFMediaBuffer> buffer;check_hresult(MFCreateMemoryBuffer(320*180*3/2,&buffer));BYTE* bytes=nullptr;check_hresult(buffer->Lock(&bytes,nullptr,nullptr));
        memset(bytes,82+(i%4)*20,320*180);memset(bytes+320*180,128,320*180/2);check_hresult(buffer->Unlock());check_hresult(buffer->SetCurrentLength(320*180*3/2));
        ComPtr<IMFSample> sample;check_hresult(MFCreateSample(&sample));check_hresult(sample->AddBuffer(buffer.Get()));check_hresult(sample->SetSampleTime(i*333333LL));check_hresult(sample->SetSampleDuration(333333));
        check_hresult(transform->ProcessInput(input_id,sample.Get(),0));++submitted;drain();
        if(r.direct_packet_count>submitted)throw hresult_error(E_UNEXPECTED);
        r.direct_max_pending=(std::max)(r.direct_max_pending,submitted-r.direct_packet_count);
    }
    r.direct_before_drain=r.direct_packet_count;
    check_hresult(transform->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM,input_id));check_hresult(transform->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN,0));drain();
    check_hresult(transform->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING,0));
    r.direct_hr=r.direct_packet_count==30?S_OK:E_FAIL;
}
