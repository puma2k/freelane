#pragma once
#include <windows.h>
#include <dwmapi.h>
#include <d3d11.h>
#include <dxgi.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mferror.h>
#include <mftransform.h>
#include <codecapi.h>
#include <icodecapi.h>
#include <wrl/client.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <chrono>
#include <mutex>
#include <memory>
#include <thread>
#include <atomic>
#include <filesystem>
#include <fstream>
#include <cstdlib>
#include "worker-setup.hpp"
#include "bgra-nv12.hpp"
#include "worker-health.hpp"
using Microsoft::WRL::ComPtr;
using namespace winrt;
using namespace winrt::Windows::Graphics::Capture;
using freelane_video::Bytes;
using freelane_video::WorkerSetup;
using Clock=std::chrono::steady_clock;
inline uint64_t clock_us(){return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::microseconds>(Clock::now().time_since_epoch()).count());}
[[noreturn]] inline void terminate_worker(){TerminateProcess(GetCurrentProcess(),3);std::abort();}
struct WinHandle {
    HANDLE value=nullptr;
    explicit WinHandle(HANDLE h=nullptr):value(h){if(h==INVALID_HANDLE_VALUE)throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));}
    ~WinHandle(){if(value)CloseHandle(value);}
    WinHandle(const WinHandle&)=delete;WinHandle& operator=(const WinHandle&)=delete;
};
inline HANDLE event_handle(){HANDLE h=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!h)throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));return h;}
inline uint64_t process_creation(DWORD pid){
    WinHandle process(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid));
    if(!process.value)throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
    FILETIME created{},exit{},kernel{},user{};
    if(!GetProcessTimes(process.value,&created,&exit,&kernel,&user))throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
    return (uint64_t(created.dwHighDateTime)<<32)|created.dwLowDateTime;
}
inline void authorize(const WorkerSetup& p){
    HWND window=reinterpret_cast<HWND>(static_cast<uintptr_t>(p.window));DWORD pid=0;
    freelane_video::target_check(IsWindow(window)&&IsWindowVisible(window)&&!IsIconic(window),freelane_video::TargetFailure::unavailable);
    freelane_video::target_check(GetWindowThreadProcessId(window,&pid)&&pid==p.pid&&process_creation(pid)==p.created,freelane_video::TargetFailure::identity);
    freelane_video::target_check(reinterpret_cast<uintptr_t>(GetPropW(window,L"FreeLane.Window.Incarnation.v9"))==p.incarnation,freelane_video::TargetFailure::identity);
}
// One overlapped request is owned until completion. Cancellation failure exits
// ONLY this helper, avoiding reuse/free of kernel-referenced request memory.
class PortOutput {
    WinHandle port_;
public:
    explicit PortOutput(const wchar_t* path):port_(CreateFileW(path,GENERIC_WRITE,0,nullptr,OPEN_EXISTING,FILE_FLAG_OVERLAPPED,nullptr)){}
    void write(const Bytes& bytes){
        WinHandle event(event_handle());OVERLAPPED request{};request.hEvent=event.value;DWORD written=0;
        BOOL ready=WriteFile(port_.value,bytes.data(),static_cast<DWORD>(bytes.size()),&written,&request);
        if(!ready){
            check_hresult(HRESULT_FROM_WIN32(GetLastError()==ERROR_IO_PENDING?ERROR_SUCCESS:GetLastError()));
            if(WaitForSingleObject(event.value,500)!=WAIT_OBJECT_0){
                CancelIoEx(port_.value,&request);
                if(WaitForSingleObject(event.value,500)!=WAIT_OBJECT_0)terminate_worker();
                // Completion/cancellation has released bytes/request; do not retry a partial record.
                throw hresult_error(HRESULT_FROM_WIN32(WAIT_TIMEOUT));
            }
            if(!GetOverlappedResult(port_.value,&request,&written,FALSE))throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
        }
        freelane_video::require(written==bytes.size(),"Partial video port write");
    }
};
// Independent watchdog covers both queued-input stalls and a blocked SDK call.
// Idle heartbeat is 1 FPS, so absence of output for 3 seconds is a failed stream.
class WorkerWatch {
    std::mutex mutex_;freelane_video::WorkerProgress progress_{clock_us()};
    uint64_t pending_=0,pictures_=0;WinHandle stop_{event_handle()};std::thread thread_;
public:
    WorkerWatch(){thread_=std::thread([this]{
        while(WaitForSingleObject(stop_.value,100)==WAIT_TIMEOUT){
            try{std::lock_guard<std::mutex> lock(mutex_);progress_.observe(pending_,pictures_,clock_us());}
            catch(...){terminate_worker();}
        }
    });}
    ~WorkerWatch(){SetEvent(stop_.value);thread_.join();}
    void progress(uint64_t pending,uint64_t pictures){std::lock_guard<std::mutex> lock(mutex_);progress_.observe(pending,pictures,clock_us());pending_=pending;pictures_=pictures;}
};
