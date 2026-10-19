// Owned SDK harness. Pixel readback exists here only, never in the renderer.
import Foundation
import AppKit
import AVFoundation
import CoreVideo
import VideoToolbox

struct ProbeFailure: Error { let message: String }
func require(_ valid: Bool, _ message: String) throws { if !valid { throw ProbeFailure(message: message) } }

@MainActor func checkSurface(_ url: URL) throws -> [String: Any] {
    let application = NSApplication.shared
    application.setActivationPolicy(.accessory); application.finishLaunching()
    let window = NSWindow(contentRect: NSRect(x: 50,y: 50,width: 600,height: 400),
        styleMask: [.titled,.closable],backing: .buffered,defer: false)
    window.isReleasedWhenClosed = false; window.makeKeyAndOrderFront(nil)
    guard let host = window.contentView else { throw ProbeFailure(message: "No owned host view") }
    defer { window.close() }
    let fixture = try loadFixture(url)
    let raw = try Data(contentsOf: url)
    try require(raw.count > 64 && raw.count <= 4*1024*1024, "Invalid captured fixture")
    let configLength = raw[8..<12].reduce(0) { ($0 << 8) | Int($1) }
    try require(configLength > 0 && configLength <= 65536 && 64+configLength < raw.count,"Invalid captured configuration")
    let configuration = Array(raw[64..<64+configLength])
    // Exercise admission before any callback can race the bounded state.
    let admission = NativeDecodeState(width:320,height:180)
    try require(admission.begin(10) == 1 && admission.begin(10) == -1 && admission.begin(11) == 1 && admission.begin(12) == 0,"Admission/PTS bounds failed")
    admission.complete(noErr,[.frameDropped],nil,CMTime(value:10,timescale:1000000))
    try require(admission.begin(12) == 1,"Busy request consumed timestamp")
    admission.close()
    try require(admission.begin(13) == -1,"Closed state accepted decode")
    let failedDisplay = NativeDecodeState(width:320,height:180)
    try require(!failedDisplay.observeDisplayStatus(.unknown) && failedDisplay.begin(1) == 1,"Healthy display rejected")
    failedDisplay.scheduled = true
    try require(failedDisplay.observeDisplayStatus(.failed) && failedDisplay.error == 5 && !failedDisplay.scheduled && failedDisplay.begin(2) == -1,"Asynchronous display failure was not terminal")
    var malformed = configuration; malformed[5] = 226
    do { _ = try nativeVideoFormat(malformed); throw ProbeFailure(message:"Multiple SPS accepted") }
    catch is NativeVideoFailure { }
    let surface = NativeVideoSurface(host: host)
    defer { surface.close() }
    try surface.configure(configuration,320,180)
    surface.place(10,10,500,300,true)
    try require(host.subviews.contains(surface.container) && surface.container.hitTest(.zero) == nil,"Surface did not attach transparently to owned view")
    var shades = Set<UInt8>()
    for (index,sample) in fixture.samples.enumerated() {
        guard let block = CMSampleBufferGetDataBuffer(sample) else { throw ProbeFailure(message:"Missing compressed data") }
        var bytes = [UInt8](repeating:0,count:CMBlockBufferGetDataLength(block))
        let count = bytes.count
        let copied = bytes.withUnsafeMutableBytes { CMBlockBufferCopyDataBytes(block,atOffset:0,dataLength:count,destination:$0.baseAddress!) }
        try require(copied == noErr,"Cannot read owned compressed sample")
        let time = CMSampleBufferGetPresentationTimeStamp(sample)
        let pts = UInt64(CMTimeGetSeconds(time)*1_000_000)
        let deadline = Date().addingTimeInterval(2)
        var accepted: Int32 = 0
        while accepted == 0 && Date() < deadline {
            accepted = bytes.withUnsafeBufferPointer { surface.submit($0,pts,index == 0) }
            if accepted == 0 { RunLoop.main.run(until: Date().addingTimeInterval(0.001)) }
        }
        try require(accepted == 1,"Native surface input stalled/rejected")
        // Drain in this disposable harness so each decoded pixel can be checked
        // before main-queue presentation consumes the latest slot.
        guard let decoder = surface.decoder, let state = surface.state else { throw ProbeFailure(message:"No decoder state") }
        try require(VTDecompressionSessionWaitForAsynchronousFrames(decoder) == noErr,"Owned decoder stalled")
        state.lock.lock(); let image = state.latest?.0; state.lock.unlock()
        guard let image = image else { throw ProbeFailure(message:"No decoded NV12 image") }
        try require(CVPixelBufferGetPixelFormatType(image) == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,"Unexpected decoded pixel type")
        try require(CVPixelBufferLockBaseAddress(image,.readOnly) == kCVReturnSuccess,"Cannot check owned pixels")
        if let pixels = CVPixelBufferGetBaseAddressOfPlane(image,0) {
            let offset = 90*CVPixelBufferGetBytesPerRowOfPlane(image,0)+160
            shades.insert(pixels.load(fromByteOffset:offset,as:UInt8.self))
        }
        CVPixelBufferUnlockBaseAddress(image,.readOnly)
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }
    RunLoop.main.run(until:Date().addingTimeInterval(0.2))
    let stats = surface.stats()
    try require(stats[0] == 30 && stats[1] == 30 && stats[2] > 0 && stats[4] == 0 && stats[5] == 1 && stats[6] == 0 && stats[7] == 1 && shades.count >= 2,"Native surface decode/presentation checks failed")
    surface.close()
    try require(!host.subviews.contains(surface.container),"Surface did not detach")
    var retired = 0
    for _ in 0..<8 {
        let next = NativeVideoSurface(host:host)
        let until = Date().addingTimeInterval(2)
        var ready = false
        while !ready && Date() < until {
            do { try next.configure(configuration,320,180); ready = true }
            catch { RunLoop.main.run(until:Date().addingTimeInterval(0.005)) }
        }
        try require(ready,"Decoder retirement budget did not release")
        let first = fixture.samples[0]
        let block = CMSampleBufferGetDataBuffer(first)!
        var bytes = [UInt8](repeating:0,count:CMBlockBufferGetDataLength(block))
        let count = bytes.count
        try require(bytes.withUnsafeMutableBytes { CMBlockBufferCopyDataBytes(block,atOffset:0,dataLength:count,destination:$0.baseAddress!) } == noErr,"Cannot copy first picture")
        try require(bytes.withUnsafeBufferPointer { next.submit($0,0,true) } == 1,"Cannot queue asynchronous picture")
        next.close(); retired += 1 // Late callbacks must not touch the removed view.
        try require(!host.subviews.contains(next.container),"Asynchronous close retained view")
    }
    RunLoop.main.run(until:Date().addingTimeInterval(0.1))
    return ["decoded":stats[1],"presented":stats[2],"hardware":stats[5] == 1,"nv12":true,
        "embedded_view":true,"asynchronous_close_cycles":retired,"pixel_variation":shades.count >= 2,
        "production_cpu_readback":false,"copy_path_measured":false,"onscreen_pixels_verified":false]
}
@main struct SurfaceProbe {
    @MainActor static func main() {
        do {
            try require(CommandLine.arguments.count == 2,"Usage: surface-probe CAPTURED_RAW_FIXTURE")
            let result = try checkSurface(URL(fileURLWithPath:CommandLine.arguments[1]))
            FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject:result,options:[.sortedKeys]))
        } catch {
            FileHandle.standardError.write(Data(((error as? ProbeFailure)?.message ?? "Native surface check failed").utf8)); exit(1)
        }
    }
}
