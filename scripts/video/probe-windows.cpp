// Experimental native SDK probe. Captures only its own painted window.
#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <codecapi.h>
#include <icodecapi.h>
#include <mftransform.h>
#include <wrl/client.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <chrono>
#include <condition_variable>
#include <filesystem>
#include <fstream>
#include <memory>
#include <mutex>
#include <vector>
#include <algorithm>
using Microsoft::WRL::ComPtr;
using namespace winrt;
using namespace winrt::Windows::Graphics::Capture;
struct Report {
    bool capture_supported=false, owned_pixels_verified=false;
    unsigned capture_frames=0, software_h264_count=0, hardware_h264_count=0, encoded_frames=0;
    bool low_latency_applied=false, zero_b_frames_applied=false;
    HRESULT capture_hr=E_PENDING, encoder_hr=E_PENDING;
    const char* d3d_device="none";
    long long encode_elapsed_ms=0, direct_first_packet_us=-1;
    unsigned direct_packet_count=0, direct_max_pending=0, direct_before_drain=0;
    unsigned raw_pictures=0,raw_configurations=0,raw_idrs=0;
    bool direct_low_latency=false, direct_zero_b=false, direct_worker_applied=false;
    ULONG direct_worker_threads=0;
    HRESULT direct_hr=E_PENDING;
};
struct Window {
    HWND handle=nullptr; HBRUSH brush=nullptr; HINSTANCE instance=GetModuleHandleW(nullptr);
    Window() {
        brush=CreateSolidBrush(RGB(40,160,80));
        WNDCLASSW cls{};cls.lpfnWndProc=DefWindowProcW;cls.hInstance=instance;
        cls.lpszClassName=L"FreeLaneOwnedVideoProbe";cls.hbrBackground=brush;
        if (!RegisterClassW(&cls)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
        handle=CreateWindowW(cls.lpszClassName,L"FreeLane owned video probe",WS_OVERLAPPEDWINDOW,
                            40,40,320,180,nullptr,nullptr,instance,nullptr);
        if (!handle) throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
        ShowWindow(handle,SW_SHOWNORMAL);UpdateWindow(handle);
    }
    ~Window(){if(handle)DestroyWindow(handle);UnregisterClassW(L"FreeLaneOwnedVideoProbe",instance);if(brush)DeleteObject(brush);}
};
struct CaptureState {
    std::mutex mutex;std::condition_variable wake;
    Direct3D11CaptureFrame first{nullptr};
};
void capture(Report& r) {
    r.capture_supported=GraphicsCaptureSession::IsSupported();
    if(!r.capture_supported){r.capture_hr=E_NOTIMPL;return;}
    Window own;
    auto factory=get_activation_factory<GraphicsCaptureItem,IGraphicsCaptureItemInterop>();
    GraphicsCaptureItem item{nullptr};
    check_hresult(factory->CreateForWindow(own.handle,guid_of<GraphicsCaptureItem>(),put_abi(item)));
    ComPtr<ID3D11Device> device;ComPtr<ID3D11DeviceContext> context;
    HRESULT hr=D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device,nullptr,&context);
    r.d3d_device="hardware_request";
    if(FAILED(hr)){r.d3d_device="warp";check_hresult(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_WARP,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device,nullptr,&context));}
    ComPtr<IDXGIDevice> dxgi;check_hresult(device.As(&dxgi));
    com_ptr<::IInspectable> inspectable;check_hresult(CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(),inspectable.put()));
    auto native=inspectable.as<winrt::Windows::Graphics::DirectX::Direct3D11::IDirect3DDevice>();
    auto pool=Direct3D11CaptureFramePool::CreateFreeThreaded(native,winrt::Windows::Graphics::DirectX::DirectXPixelFormat::B8G8R8A8UIntNormalized,2,item.Size());
    auto state=std::make_shared<CaptureState>();
    auto token=pool.FrameArrived([state](auto const& source,auto const&) {
        try {auto frame=source.TryGetNextFrame();if(!frame)return;
            std::lock_guard<std::mutex> guard(state->mutex);
            if(!state->first){state->first=frame;state->wake.notify_one();}
        }catch(...){ /* Bounded outer wait reports unavailable; no foreign windows are captured. */ }
    });
    auto session=pool.CreateCaptureSession(item);session.StartCapture();
    const auto end=std::chrono::steady_clock::now()+std::chrono::seconds(5);
    while(std::chrono::steady_clock::now()<end) {
        MSG msg{};while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)){TranslateMessage(&msg);DispatchMessageW(&msg);}
        std::unique_lock<std::mutex> lock(state->mutex);
        if(state->first)break;
        state->wake.wait_for(lock,std::chrono::milliseconds(20));
    }
    pool.FrameArrived(token);
    Direct3D11CaptureFrame frame{nullptr};{std::lock_guard<std::mutex> lock(state->mutex);frame=state->first;state->first=nullptr;}
    if(frame) {
        auto access=frame.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>();
        ComPtr<ID3D11Texture2D> texture;check_hresult(access->GetInterface(__uuidof(ID3D11Texture2D),reinterpret_cast<void**>(texture.GetAddressOf())));
        D3D11_TEXTURE2D_DESC desc{};texture->GetDesc(&desc);
        if(desc.Width==0||desc.Height==0||desc.Width>1920||desc.Height>1080)throw hresult_error(E_INVALIDARG);
        desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;desc.MiscFlags=0;
        ComPtr<ID3D11Texture2D> staging;check_hresult(device->CreateTexture2D(&desc,nullptr,&staging));
        context->CopyResource(staging.Get(),texture.Get());
        D3D11_MAPPED_SUBRESOURCE mapped{};check_hresult(context->Map(staging.Get(),0,D3D11_MAP_READ,0,&mapped));
        const auto* pixel=static_cast<const unsigned char*>(mapped.pData)+(desc.Height/2)*mapped.RowPitch+(desc.Width/2)*4;
        r.owned_pixels_verified=pixel[1]>130&&pixel[1]<190&&pixel[0]>50&&pixel[0]<110&&pixel[2]<80;
        context->Unmap(staging.Get(),0);r.capture_frames=1;r.capture_hr=S_OK;frame.Close();
    }else r.capture_hr=HRESULT_FROM_WIN32(WAIT_TIMEOUT);
    session.Close();pool.Close();
}
unsigned encoders(UINT32 flags) {
    MFT_REGISTER_TYPE_INFO type{MFMediaType_Video,MFVideoFormat_H264};IMFActivate** entries=nullptr;UINT32 count=0;
    check_hresult(MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,flags,nullptr,&type,&entries,&count));
    for(UINT32 i=0;i<count;++i)entries[i]->Release();CoTaskMemFree(entries);
    return count;
}
bool codec_bool(ICodecAPI* api,const GUID& key,bool boolean) {
    VARIANT request;VariantInit(&request);request.vt=boolean?VT_BOOL:VT_UI4;
    if(boolean)request.boolVal=VARIANT_TRUE;else request.ulVal=0;
    const HRESULT set=api->SetValue(&key,&request);VariantClear(&request);
    VARIANT actual;VariantInit(&actual);const HRESULT get=api->GetValue(&key,&actual);
    const bool valid=SUCCEEDED(set)&&SUCCEEDED(get)&&(boolean?(actual.vt==VT_BOOL&&actual.boolVal==VARIANT_TRUE):((actual.vt==VT_UI4&&actual.ulVal==0)||(actual.vt==VT_I4&&actual.lVal==0)));
    VariantClear(&actual);return valid;
}
bool codec_uint(ICodecAPI* api,const GUID& key,ULONG wanted,ULONG& actual_value) {
    VARIANT request;VariantInit(&request);request.vt=VT_UI4;request.ulVal=wanted;
    HRESULT set=api->SetValue(&key,&request);VariantClear(&request);
    VARIANT actual;VariantInit(&actual);HRESULT get=api->GetValue(&key,&actual);
    bool number=actual.vt==VT_UI4||(actual.vt==VT_I4&&actual.lVal>=0);
    if(number)actual_value=actual.vt==VT_UI4?actual.ulVal:static_cast<ULONG>(actual.lVal);
    VariantClear(&actual);return SUCCEEDED(set)&&SUCCEEDED(get)&&number&&actual_value==wanted;
}
#include "direct-encoder.hpp"
void encode(Report& r,const std::filesystem::path& path) {
    r.software_h264_count=encoders(MFT_ENUM_FLAG_SYNCMFT|MFT_ENUM_FLAG_SORTANDFILTER);
    r.hardware_h264_count=encoders(MFT_ENUM_FLAG_HARDWARE|MFT_ENUM_FLAG_SORTANDFILTER);
    ComPtr<IMFAttributes> attrs;check_hresult(MFCreateAttributes(&attrs,4));
    check_hresult(attrs->SetUINT32(MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,FALSE));
    check_hresult(attrs->SetUINT32(MF_SINK_WRITER_DISABLE_THROTTLING,TRUE));check_hresult(attrs->SetUINT32(MF_LOW_LATENCY,TRUE));
    ComPtr<IMFSinkWriter> writer;check_hresult(MFCreateSinkWriterFromURL(path.c_str(),nullptr,attrs.Get(),&writer));
    ComPtr<IMFMediaType> output;check_hresult(MFCreateMediaType(&output));
    check_hresult(output->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video));check_hresult(output->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_H264));
    check_hresult(output->SetUINT32(MF_MT_AVG_BITRATE,1000000));check_hresult(output->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive));
    check_hresult(output->SetUINT32(MF_MT_MPEG2_PROFILE,66)); // H.264 baseline.
    check_hresult(MFSetAttributeSize(output.Get(),MF_MT_FRAME_SIZE,320,180));check_hresult(MFSetAttributeRatio(output.Get(),MF_MT_FRAME_RATE,30,1));check_hresult(MFSetAttributeRatio(output.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1));
    DWORD stream=0;check_hresult(writer->AddStream(output.Get(),&stream));
    ComPtr<IMFMediaType> input;check_hresult(MFCreateMediaType(&input));check_hresult(input->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video));check_hresult(input->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_NV12));
    check_hresult(input->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive));check_hresult(MFSetAttributeSize(input.Get(),MF_MT_FRAME_SIZE,320,180));check_hresult(MFSetAttributeRatio(input.Get(),MF_MT_FRAME_RATE,30,1));check_hresult(MFSetAttributeRatio(input.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1));
    check_hresult(writer->SetInputMediaType(stream,input.Get(),nullptr));check_hresult(writer->BeginWriting());
    ComPtr<IMFSinkWriterEx> extended;
    if(SUCCEEDED(writer.As(&extended)))for(DWORD i=0;i<4;++i){GUID category{};ComPtr<IMFTransform> transform;
        if(FAILED(extended->GetTransformForStream(stream,i,&category,&transform)))break;
        if(category==MFT_CATEGORY_VIDEO_ENCODER){ComPtr<ICodecAPI> api;if(SUCCEEDED(transform.As(&api))){r.low_latency_applied=codec_bool(api.Get(),CODECAPI_AVLowLatencyMode,true);r.zero_b_frames_applied=codec_bool(api.Get(),CODECAPI_AVEncMPVDefaultBPictureCount,false);}break;}
    }
    const auto started=std::chrono::steady_clock::now();
    for(unsigned i=0;i<30;++i){
        ComPtr<IMFMediaBuffer> buffer;check_hresult(MFCreateMemoryBuffer(320*180*3/2,&buffer));BYTE* bytes=nullptr;check_hresult(buffer->Lock(&bytes,nullptr,nullptr));
        memset(bytes,82+(i%4)*20,320*180);memset(bytes+320*180,128,320*180/2);check_hresult(buffer->Unlock());check_hresult(buffer->SetCurrentLength(320*180*3/2));
        ComPtr<IMFSample> sample;check_hresult(MFCreateSample(&sample));check_hresult(sample->AddBuffer(buffer.Get()));check_hresult(sample->SetSampleTime(i*333333LL));check_hresult(sample->SetSampleDuration(333333));check_hresult(writer->WriteSample(stream,sample.Get()));
    }
    check_hresult(writer->Finalize());MF_SINK_WRITER_STATISTICS stats{};stats.cb=sizeof(stats);check_hresult(writer->GetStatistics(stream,&stats));
    r.encoded_frames=static_cast<unsigned>(stats.qwNumSamplesEncoded);r.encode_elapsed_ms=std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now()-started).count();
    if(std::filesystem::file_size(path)>4*1024*1024)throw hresult_error(E_INVALIDARG);
    r.encoder_hr=S_OK;
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=2||!CreateDirectoryW(argv[1],nullptr))return 2;
    Report r;bool mf=false, apartment=false;
    try{init_apartment(apartment_type::multi_threaded);apartment=true;check_hresult(MFStartup(MF_VERSION));mf=true;
        try{capture(r);}catch(const hresult_error& e){r.capture_hr=e.code().value;}catch(...){r.capture_hr=E_FAIL;}
        try{encode(r,std::filesystem::path(argv[1])/L"fixture.mp4");}catch(const hresult_error& e){r.encoder_hr=e.code().value;}catch(...){r.encoder_hr=E_FAIL;}
        try{direct_encode(r,std::filesystem::path(argv[1]));}catch(const hresult_error& e){r.direct_hr=e.code().value;}catch(...){r.direct_hr=E_FAIL;}
    }catch(...){r.encoder_hr=E_FAIL;}
    if(mf)MFShutdown();if(apartment)uninit_apartment();
#ifdef _M_ARM64
    const char* arch="arm64";
#else
    const char* arch="x86_64";
#endif
    std::ofstream out(std::filesystem::path(argv[1])/L"report.json");
    out<<std::boolalpha<<"{\"schema\":1,\"platform\":\"windows\",\"architecture\":\""<<arch<<"\",\"capture_supported\":"<<r.capture_supported<<",\"capture_frames\":"<<r.capture_frames<<",\"owned_pixels_verified\":"<<r.owned_pixels_verified<<",\"software_h264_count\":"<<r.software_h264_count<<",\"hardware_h264_count\":"<<r.hardware_h264_count<<",\"encoded_frames\":"<<r.encoded_frames<<",\"low_latency_applied\":"<<r.low_latency_applied<<",\"zero_b_frames_applied\":"<<r.zero_b_frames_applied<<",\"raw_packet_latency_measured\":"<<(SUCCEEDED(r.direct_hr)&&r.direct_packet_count==30)<<",\"capture_hresult\":"<<static_cast<long>(r.capture_hr)<<",\"encoder_hresult\":"<<static_cast<long>(r.encoder_hr)<<",\"capture_validation_cpu_readback\":true,\"d3d_device_kind\":\""<<r.d3d_device<<"\",\"reference_encoder_hardware_requested\":false,\"encode_elapsed_ms\":"<<r.encode_elapsed_ms<<",\"direct_packet_count\":"<<r.direct_packet_count<<",\"direct_max_pending_frames\":"<<r.direct_max_pending<<",\"direct_packets_before_drain\":"<<r.direct_before_drain<<",\"direct_first_packet_us\":"<<r.direct_first_packet_us<<",\"direct_input_cadence_fps\":30,\"direct_low_latency_applied\":"<<r.direct_low_latency<<",\"direct_zero_b_frames_applied\":"<<r.direct_zero_b<<",\"direct_hresult\":"<<static_cast<long>(r.direct_hr)<<",\"direct_worker_threads\":"<<r.direct_worker_threads<<",\"direct_worker_threads_applied\":"<<r.direct_worker_applied<<",\"raw_stream_pictures\":"<<r.raw_pictures<<",\"raw_stream_configurations\":"<<r.raw_configurations<<",\"raw_stream_idrs\":"<<r.raw_idrs<<"}\n";
    return out?0:3;
}
