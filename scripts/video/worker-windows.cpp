// Native worker: no app launch, no shell, no public network endpoint.
#include "worker-capture.hpp"
#include "worker-codec.hpp"
#include <string>
inline void status_byte(char c){DWORD n=0;if(!WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),&c,1,&n,nullptr)||n!=1)throw std::runtime_error("Control peer disconnected");}
inline void read_exact(void* data,DWORD size){
    auto* bytes=static_cast<uint8_t*>(data);DWORD at=0;
    while(at<size){DWORD n=0;if(!ReadFile(GetStdHandle(STD_INPUT_HANDLE),bytes+at,size-at,&n,nullptr)||n==0)throw std::runtime_error("Control input ended");at+=n;}
}
class Commands {
    std::atomic<bool> closing_{false};std::thread reader_;
public:
    std::atomic<bool> stop{false},key{false},bad{false};WinHandle wake{event_handle()};
    Commands(){reader_=std::thread([this]{
        while(!closing_){char c=0;DWORD n=0;BOOL ok=ReadFile(GetStdHandle(STD_INPUT_HANDLE),&c,1,&n,nullptr);
            if(!ok||n==0){stop=true;SetEvent(wake.value);return;}
            if(c=='K')key=true;else{stop=true;if(c!='X')bad=true;}
            SetEvent(wake.value);if(stop)return;
        }
    });}
    ~Commands(){
        closing_=true;auto end=Clock::now()+std::chrono::seconds(1);
        do{CancelSynchronousIo(reader_.native_handle());
            if(WaitForSingleObject(reader_.native_handle(),10)==WAIT_OBJECT_0){reader_.join();return;}
        }while(Clock::now()<end);
        ExitProcess(3); // Only the helper, never the captured application.
    }
};
struct OwnedWindow {
    HWND handle=nullptr;HBRUSH brush=nullptr;std::wstring name;
    OwnedWindow(){
        static unsigned serial=0;name=L"FreeLaneVideoWorkerOwned"+std::to_wstring(++serial);
        brush=CreateSolidBrush(RGB(70,70,70));freelane_video::require(brush!=nullptr,"Cannot create test brush");
        WNDCLASSW cls{};cls.lpfnWndProc=DefWindowProcW;cls.hInstance=GetModuleHandleW(nullptr);cls.lpszClassName=name.c_str();cls.hbrBackground=brush;
        freelane_video::require(RegisterClassW(&cls)!=0,"Cannot register owned window");
        handle=CreateWindowW(name.c_str(),L"FreeLane owned worker test",WS_OVERLAPPEDWINDOW,40,40,320,180,nullptr,nullptr,cls.hInstance,nullptr);
        freelane_video::require(handle!=nullptr,"Cannot create owned window");
        freelane_video::require(SetPropW(handle,L"FreeLane.Window.Incarnation.v9",reinterpret_cast<HANDLE>(77))!=FALSE,"Cannot set test identity");
        ShowWindow(handle,SW_SHOWNORMAL);UpdateWindow(handle);
    }
    ~OwnedWindow(){if(handle)DestroyWindow(handle);UnregisterClassW(name.c_str(),GetModuleHandleW(nullptr));if(brush)DeleteObject(brush);}
    WorkerSetup setup()const{
        WorkerSetup p;p.window=reinterpret_cast<uintptr_t>(handle);p.pid=GetCurrentProcessId();p.created=process_creation(p.pid);p.incarnation=77;
        p.binding.nonce.fill(7);p.binding.stream=1;p.binding.generation=1;p.width=320;p.height=180;return p;
    }
    void paint(unsigned index){
        const unsigned shade=70+(index%4)*30;auto next=CreateSolidBrush(RGB(shade,shade,shade));freelane_video::require(next!=nullptr,"Cannot repaint owned window");
        SetClassLongPtrW(handle,GCLP_HBRBACKGROUND,reinterpret_cast<LONG_PTR>(next));auto previous=brush;brush=next;DeleteObject(previous);
        InvalidateRect(handle,nullptr,TRUE);UpdateWindow(handle);
    }
};
struct RunStats {uint64_t pictures=0,captured=0;unsigned idrs=0,pending=0,width=0,height=0;};
RunStats run(const WorkerSetup& setup,const std::function<void(const Bytes&)>& sink,OwnedWindow* owned=nullptr,int lifecycle=0){
    authorize(setup);WindowCapture capture(setup);bool started=false;
    WindowCodec codec(setup.binding,capture.width,capture.height,sink,[&]{started=true;if(!owned)status_byte('S');});
    std::unique_ptr<Commands> commands;
    if(!owned){status_byte('R');char go=0;read_exact(&go,1);freelane_video::require(go=='G',"Invalid start authorization");authorize(setup);commands=std::make_unique<Commands>();}
    const auto origin=Clock::now();auto next=origin,last_input=origin,first_deadline=origin+std::chrono::seconds(3);
    Bytes cache;uint64_t submitted=0,last_pts=0;bool dirty=false,forced=false;
    for(;;){
        if(owned){MSG msg{};while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)){TranslateMessage(&msg);DispatchMessageW(&msg);}}
        if(commands){freelane_video::require(!commands->bad,"Invalid helper command");if(commands->stop)break;}
        authorize(setup);codec.drain();
        auto now=Clock::now();freelane_video::require(started||now<first_deadline,"First encoded picture timed out");
        if(owned&&submitted>=90){codec.finish();break;}
        if(now>=next){
            auto frame=capture.take();
            if(!frame.empty()){
                if(owned){
                    unsigned shade=70+(static_cast<unsigned>(submitted)%4)*30;int expected=((220*int(shade)+128)>>8)+16;
                    if(std::abs(int(frame[(capture.height/2)*capture.width+capture.width/2])-expected)>3)frame.clear();
                }
                if(!frame.empty()&&frame!=cache){cache=std::move(frame);dirty=true;}
            }
            bool key=commands&&commands->key.load();
            bool heartbeat=now-last_input>=std::chrono::seconds(1);
            if(!cache.empty()&&codec.pending()<2&&(dirty||(!owned&&submitted<2)||key||(!owned&&heartbeat))){
                if(owned&&submitted==45&&!forced){codec.keyframe();forced=true;}
                if(key){codec.keyframe();commands->key=false;status_byte('K');}
                uint64_t pts=static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::microseconds>(now-origin).count());
                if(submitted)pts=(std::max)(pts,last_pts+1);
                authorize(setup);codec.submit(cache,pts);last_pts=pts;++submitted;dirty=false;last_input=now;
                if(owned){
                    if(lifecycle&&submitted==10){
                        if(lifecycle==1)SetPropW(owned->handle,L"FreeLane.Window.Incarnation.v9",reinterpret_cast<HANDLE>(78));
                        if(lifecycle==2)SetWindowPos(owned->handle,nullptr,0,0,420,240,SWP_NOMOVE|SWP_NOZORDER);
                        if(lifecycle==3)DestroyWindow(owned->handle);
                    }
                    owned->paint(static_cast<unsigned>(submitted));
                }
            }
            next=now+std::chrono::microseconds(33334);
        }
        if(owned)freelane_video::require(Clock::now()-origin<std::chrono::seconds(12),"Owned worker test stalled");
        HANDLE events[2]={capture.wake(),commands?commands->wake.value:nullptr};
        auto deadline=next;
        if(!dirty&&submitted>=2&&(!owned))deadline=last_input+std::chrono::seconds(1);
        auto wait=std::chrono::duration_cast<std::chrono::milliseconds>(deadline-Clock::now()).count();
        DWORD timeout=static_cast<DWORD>((std::clamp)(wait,1ll,100ll));
        if(codec.pending()>=2)timeout=(std::min)(timeout,DWORD(5));
        WaitForMultipleObjects(commands?2:1,events,FALSE,timeout);
    }
    return {codec.pictures(),capture.frames,codec.idrs,codec.max_pending,capture.width,capture.height};
}
void stall_test(){
    auto name=L"\\\\.\\pipe\\FreeLaneVideoStall"+std::to_wstring(GetCurrentProcessId());
    WinHandle server(CreateNamedPipeW(name.c_str(),PIPE_ACCESS_INBOUND|FILE_FLAG_OVERLAPPED,PIPE_TYPE_BYTE|PIPE_WAIT,1,4096,4096,0,nullptr));
    PortOutput output(name.c_str());const auto started=Clock::now();bool failed=false;
    status_byte('T');try{output.write(Bytes(2*1024*1024,7));}catch(...){failed=true;}
    freelane_video::require(failed&&Clock::now()-started<std::chrono::seconds(2),"Stalled output did not fail within budget");
    status_byte('F');
}
int wmain(int argc,wchar_t** argv){
    bool apartment=false,mf=false;int result=3;
    try{
        init_apartment(apartment_type::multi_threaded);apartment=true;check_hresult(MFStartup(MF_VERSION));mf=true;
        if(argc==2&&std::wstring(argv[1])==L"--owned-stall-test"){stall_test();result=0;}
        else if(argc==3&&std::wstring(argv[1])==L"--owned-self-test"){
            freelane_video::require(CreateDirectoryW(argv[2],nullptr)!=FALSE,"Refuse existing test destination");
            auto directory=std::filesystem::path(argv[2]);RunStats stats;
            {OwnedWindow window;OwnedWindow unrelated;SetWindowPos(unrelated.handle,nullptr,700,40,0,0,SWP_NOSIZE|SWP_NOZORDER);std::ofstream output(directory/L"worker-packets.flv",std::ios::binary);
                freelane_video::require(bool(output),"Cannot create owned packet file");
                stats=run(window.setup(),[&](const Bytes& bytes){output.write(reinterpret_cast<const char*>(bytes.data()),bytes.size());freelane_video::require(bool(output),"Owned file output failed");},&window);
                freelane_video::require(IsWindow(window.handle)&&IsWindow(unrelated.handle),"Worker cleanup closed an application");}
            unsigned failures=0;
            for(int mode=1;mode<=3;++mode){OwnedWindow window;OwnedWindow unrelated;SetWindowPos(unrelated.handle,nullptr,700,40,0,0,SWP_NOSIZE|SWP_NOZORDER);try{run(window.setup(),[](const Bytes&){},&window,mode);}catch(...){++failures;}
                freelane_video::require(IsWindow(unrelated.handle)!=FALSE,"Lifecycle cleanup closed unrelated app");}
            std::ofstream report(directory/L"worker-report.json");report<<"{\"pictures\":"<<stats.pictures<<",\"captured_frames\":"<<stats.captured<<",\"idrs\":"<<stats.idrs<<",\"max_pending\":"<<stats.pending<<",\"width\":"<<stats.width<<",\"height\":"<<stats.height<<",\"lifecycle_failures\":"<<failures<<"}\n";
            freelane_video::require(stats.pictures==90&&stats.captured>=90&&stats.idrs>=2&&stats.pending<=2&&failures==3&&bool(report),"Owned persistent stream checks failed");result=0;
        }else if(argc==1){
            Bytes bytes(80);read_exact(bytes.data(),80);auto setup=freelane_video::worker_setup(bytes);authorize(setup);
            PortOutput output(L"\\\\.\\Global\\org.freelane.video.0");run(setup,[&](const Bytes& packet){output.write(packet);});result=0;
        }
    }catch(...){ /* Fixed exit status only; never print pixels, paths or user text. */ }
    if(mf)MFShutdown();if(apartment)uninit_apartment();return result;
}
