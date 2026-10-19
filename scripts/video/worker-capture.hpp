#pragma once
#include "worker-native.hpp"
class WindowCapture {
    struct Latest {std::mutex mutex;Direct3D11CaptureFrame frame{nullptr};bool closed=false;WinHandle wake{event_handle()};};
    WorkerSetup target_;
    ComPtr<ID3D11Device> device_;ComPtr<ID3D11DeviceContext> context_;ComPtr<ID3D11Texture2D> staging_;
    GraphicsCaptureItem item_{nullptr};Direct3D11CaptureFramePool pool_{nullptr};GraphicsCaptureSession session_{nullptr};
    std::shared_ptr<Latest> latest_=std::make_shared<Latest>();
    event_token frame_token_{},close_token_{};bool subscribed_=false,closed_subscribed_=false;
    unsigned source_width_=0,source_height_=0;
    struct Client {unsigned x,y,width,height,dpi;
        bool operator==(const Client& other)const{return x==other.x&&y==other.y&&width==other.width&&height==other.height&&dpi==other.dpi;}
    } client_{};
    Client client_geometry()const {
        auto window=reinterpret_cast<HWND>(static_cast<uintptr_t>(target_.window));
        RECT bounds{},client{};POINT origin{};
        auto valid=GetClientRect(window,&client)!=FALSE&&ClientToScreen(window,&origin)!=FALSE&&
            SUCCEEDED(DwmGetWindowAttribute(window,DWMWA_EXTENDED_FRAME_BOUNDS,&bounds,sizeof(bounds)));
        const auto dpi=GetDpiForWindow(window);
        freelane_video::target_check(valid&&dpi>0&&bounds.right>bounds.left&&bounds.bottom>bounds.top&&
            bounds.right-bounds.left==int(source_width_)&&bounds.bottom-bounds.top==int(source_height_)&&
            client.left==0&&client.top==0&&client.right>0&&client.bottom>0&&
            origin.x>=bounds.left&&origin.y>=bounds.top&&
            client.right<=bounds.right-origin.x&&client.bottom<=bounds.bottom-origin.y,
            freelane_video::TargetFailure::resize);
        return {unsigned(origin.x-bounds.left),unsigned(origin.y-bounds.top),unsigned(client.right),unsigned(client.bottom),dpi};
    }
public:
    unsigned width=0,height=0;uint64_t frames=0;
    explicit WindowCapture(const WorkerSetup& target):target_(target){
        authorize(target_);freelane_video::require(GraphicsCaptureSession::IsSupported(),"WGC unavailable");
        auto factory=get_activation_factory<GraphicsCaptureItem,IGraphicsCaptureItemInterop>();
        check_hresult(factory->CreateForWindow(reinterpret_cast<HWND>(static_cast<uintptr_t>(target.window)),guid_of<GraphicsCaptureItem>(),put_abi(item_)));
        auto size=item_.Size();freelane_video::require(size.Width>0&&size.Height>0&&size.Width<=1920&&size.Height<=1080,"Capture bounds unsupported");
        source_width_=size.Width;source_height_=size.Height;
        client_=client_geometry();
        const double scale=(std::min)({1.0,double(target.width)/client_.width,double(target.height)/client_.height});
        width=static_cast<unsigned>(client_.width*scale)/2*2;height=static_cast<unsigned>(client_.height*scale)/2*2;
        freelane_video::valid_dimensions(width,height);
        HRESULT hr=D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device_,nullptr,&context_);
        if(FAILED(hr))check_hresult(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_WARP,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device_,nullptr,&context_));
        ComPtr<IDXGIDevice> dxgi;check_hresult(device_.As(&dxgi));
        com_ptr<::IInspectable> inspectable;check_hresult(CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(),inspectable.put()));
        auto native=inspectable.as<winrt::Windows::Graphics::DirectX::Direct3D11::IDirect3DDevice>();
        pool_=Direct3D11CaptureFramePool::CreateFreeThreaded(native,winrt::Windows::Graphics::DirectX::DirectXPixelFormat::B8G8R8A8UIntNormalized,2,size);
        frame_token_=pool_.FrameArrived([state=latest_](auto const& source,auto const&){
            try {auto frame=source.TryGetNextFrame();if(!frame)return;std::lock_guard<std::mutex> lock(state->mutex);
                if(state->closed){frame.Close();return;}if(state->frame)state->frame.Close();state->frame=frame;SetEvent(state->wake.value);
            }catch(...){std::lock_guard<std::mutex> lock(state->mutex);state->closed=true;SetEvent(state->wake.value);}
        });subscribed_=true;
        close_token_=item_.Closed([state=latest_](auto const&,auto const&){std::lock_guard<std::mutex> lock(state->mutex);state->closed=true;SetEvent(state->wake.value);});closed_subscribed_=true;
        session_=pool_.CreateCaptureSession(item_);authorize(target_);session_.StartCapture();
    }
    ~WindowCapture(){
        {std::lock_guard<std::mutex> lock(latest_->mutex);latest_->closed=true;
            if(latest_->frame){try{latest_->frame.Close();}catch(...){}latest_->frame=nullptr;}}
        try{if(subscribed_)pool_.FrameArrived(frame_token_);}catch(...){}
        try{if(closed_subscribed_)item_.Closed(close_token_);}catch(...){}
        try{if(session_)session_.Close();}catch(...){}try{if(pool_)pool_.Close();}catch(...){}
    }
    HANDLE wake()const{return latest_->wake.value;}
    Bytes take(){
        authorize(target_);Direct3D11CaptureFrame frame{nullptr};
        {std::lock_guard<std::mutex> lock(latest_->mutex);freelane_video::target_check(!latest_->closed,freelane_video::TargetFailure::unavailable);frame=latest_->frame;latest_->frame=nullptr;}
        if(!frame)return {};
        try {
            auto content=frame.ContentSize();freelane_video::target_check(content.Width==int(source_width_)&&content.Height==int(source_height_),freelane_video::TargetFailure::resize);
            freelane_video::target_check(client_geometry()==client_,freelane_video::TargetFailure::resize);
            auto access=frame.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>();
            ComPtr<ID3D11Texture2D> texture;check_hresult(access->GetInterface(__uuidof(ID3D11Texture2D),reinterpret_cast<void**>(texture.GetAddressOf())));
            D3D11_TEXTURE2D_DESC desc{};texture->GetDesc(&desc);
            freelane_video::require(desc.Width==source_width_&&desc.Height==source_height_&&desc.Format==DXGI_FORMAT_B8G8R8A8_UNORM&&desc.MipLevels==1&&desc.ArraySize==1&&desc.SampleDesc.Count==1,"Unexpected capture texture");
            if(!staging_){desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;desc.MiscFlags=0;check_hresult(device_->CreateTexture2D(&desc,nullptr,&staging_));}
            context_->CopyResource(staging_.Get(),texture.Get());D3D11_MAPPED_SUBRESOURCE mapped{};
            check_hresult(context_->Map(staging_.Get(),0,D3D11_MAP_READ,0,&mapped));Bytes result;
            try {result=freelane_video::bgra_nv12_crop(static_cast<const uint8_t*>(mapped.pData),size_t(mapped.RowPitch)*source_height_,mapped.RowPitch,source_width_,source_height_,client_.x,client_.y,client_.width,client_.height,width,height);}
            catch(...){context_->Unmap(staging_.Get(),0);throw;}
            context_->Unmap(staging_.Get(),0);frame.Close();++frames;authorize(target_);freelane_video::target_check(client_geometry()==client_,freelane_video::TargetFailure::resize);return result;
        }catch(...){try{frame.Close();}catch(...){}throw;}
    }
};
