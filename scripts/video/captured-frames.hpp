#pragma once
// Owned-window validation only. No foreign HWNDs or production authorization.
#include "bgra-nv12.hpp"
#include <cstdlib>
class CapturedFrames {
    struct Latest {
        std::mutex mutex;
        Direct3D11CaptureFrame frame{nullptr};
        bool closed=false;
    };
    Window own_;
    ComPtr<ID3D11Device> device_;
    ComPtr<ID3D11DeviceContext> context_;
    Direct3D11CaptureFramePool pool_{nullptr};
    GraphicsCaptureSession session_{nullptr};
    std::shared_ptr<Latest> state_=std::make_shared<Latest>();
    event_token token_{};
    bool subscribed_=false;
public:
    unsigned captured=0;
    CapturedFrames() {
        freelane_video::require(GraphicsCaptureSession::IsSupported(),"WGC is unavailable");
        auto factory=get_activation_factory<GraphicsCaptureItem,IGraphicsCaptureItemInterop>();
        GraphicsCaptureItem item{nullptr};
        check_hresult(factory->CreateForWindow(own_.handle,guid_of<GraphicsCaptureItem>(),put_abi(item)));
        HRESULT hr=D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device_,nullptr,&context_);
        if(FAILED(hr))check_hresult(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_WARP,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device_,nullptr,&context_));
        ComPtr<IDXGIDevice> dxgi;check_hresult(device_.As(&dxgi));
        com_ptr<::IInspectable> inspectable;check_hresult(CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(),inspectable.put()));
        auto native=inspectable.as<winrt::Windows::Graphics::DirectX::Direct3D11::IDirect3DDevice>();
        pool_=Direct3D11CaptureFramePool::CreateFreeThreaded(native,winrt::Windows::Graphics::DirectX::DirectXPixelFormat::B8G8R8A8UIntNormalized,2,item.Size());
        token_=pool_.FrameArrived([state=state_](auto const& source,auto const&){
            try {
                auto frame=source.TryGetNextFrame();if(!frame)return;
                std::lock_guard<std::mutex> guard(state->mutex);
                if(state->closed){frame.Close();return;}
                if(state->frame)state->frame.Close();
                state->frame=frame;
            }catch(...){ /* Finite caller timeout fails the probe. */ }
        });
        subscribed_=true;session_=pool_.CreateCaptureSession(item);session_.StartCapture();
    }
    CapturedFrames(const CapturedFrames&)=delete;
    CapturedFrames& operator=(const CapturedFrames&)=delete;
    ~CapturedFrames() {
        {std::lock_guard<std::mutex> guard(state_->mutex);state_->closed=true;
            if(state_->frame){try{state_->frame.Close();}catch(...){}state_->frame=nullptr;}}
        try{if(subscribed_)pool_.FrameArrived(token_);}catch(...){}
        try{if(session_)session_.Close();}catch(...){}
        try{if(pool_)pool_.Close();}catch(...){}
    }
    freelane_video::Bytes next(unsigned index) {
        freelane_video::require(IsWindow(own_.handle)!=FALSE,"Owned capture window closed");
        const unsigned shade=70+(index%4)*30;
        HBRUSH brush=CreateSolidBrush(RGB(shade,shade,shade));
        if(!brush)throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
        // Update the registered class brush so WM_ERASEBKGND paints real pixels.
        SetClassLongPtrW(own_.handle,GCLP_HBRBACKGROUND,reinterpret_cast<LONG_PTR>(brush));
        auto previous=own_.brush;own_.brush=brush;if(previous)DeleteObject(previous);
        InvalidateRect(own_.handle,nullptr,TRUE);UpdateWindow(own_.handle);
        const auto end=std::chrono::steady_clock::now()+std::chrono::seconds(3);
        while(std::chrono::steady_clock::now()<end){
            MSG msg{};while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)){TranslateMessage(&msg);DispatchMessageW(&msg);}
            Direct3D11CaptureFrame frame{nullptr};
            {std::lock_guard<std::mutex> guard(state_->mutex);frame=state_->frame;state_->frame=nullptr;}
            if(!frame){std::this_thread::sleep_for(std::chrono::milliseconds(2));continue;}
            try {
                auto access=frame.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>();
                ComPtr<ID3D11Texture2D> texture;check_hresult(access->GetInterface(__uuidof(ID3D11Texture2D),reinterpret_cast<void**>(texture.GetAddressOf())));
                D3D11_TEXTURE2D_DESC desc{};texture->GetDesc(&desc);
                freelane_video::require(desc.Width>0&&desc.Height>0&&desc.Width<=1920&&desc.Height<=1080&&desc.Format==DXGI_FORMAT_B8G8R8A8_UNORM&&desc.ArraySize==1&&desc.MipLevels==1&&desc.SampleDesc.Count==1,"Unexpected WGC texture");
                const auto content=frame.ContentSize();
                freelane_video::require(content.Width==static_cast<int>(desc.Width)&&content.Height==static_cast<int>(desc.Height),"Owned window resized");
                desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;desc.MiscFlags=0;
                ComPtr<ID3D11Texture2D> staging;check_hresult(device_->CreateTexture2D(&desc,nullptr,&staging));
                context_->CopyResource(staging.Get(),texture.Get());
                D3D11_MAPPED_SUBRESOURCE mapped{};check_hresult(context_->Map(staging.Get(),0,D3D11_MAP_READ,0,&mapped));
                freelane_video::Bytes pixels;bool matching=false;
                try {
                    freelane_video::require(mapped.pData&&mapped.RowPitch>=desc.Width*4,"Invalid mapped WGC surface");
                    const auto* center=static_cast<const uint8_t*>(mapped.pData)+(desc.Height/2)*size_t(mapped.RowPitch)+(desc.Width/2)*4;
                    matching=true;for(unsigned channel=0;channel<3;++channel)matching=matching&&std::abs(int(center[channel])-int(shade))<=3;
                    if(matching)pixels=freelane_video::bgra_nv12(static_cast<const uint8_t*>(mapped.pData),size_t(mapped.RowPitch)*desc.Height,mapped.RowPitch,desc.Width,desc.Height);
                }catch(...){context_->Unmap(staging.Get(),0);throw;}
                context_->Unmap(staging.Get(),0);frame.Close();
                if(matching){++captured;return pixels;}
            }catch(...){try{frame.Close();}catch(...){}throw;}
        }
        throw hresult_error(HRESULT_FROM_WIN32(WAIT_TIMEOUT));
    }
};
