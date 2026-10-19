// Native renderer only. The guest/controller authorizes capture, never video bytes.
import Foundation
import AppKit
import AVFoundation
import VideoToolbox
import CoreVideo

enum NativeVideoFailure: Error { case invalid }
func videoCheck(_ valid: Bool) throws { if !valid { throw NativeVideoFailure.invalid } }

final class DecoderBudget: @unchecked Sendable {
    static let shared = DecoderBudget()
    private let lock = NSLock()
    private var reserved = false
    func take() -> Bool { lock.lock(); defer { lock.unlock() }; if reserved { return false }; reserved = true; return true }
    func release() { lock.lock(); reserved = false; lock.unlock() }
}

@MainActor final class VideoContainer: NSView {
    let display = AVSampleBufferDisplayLayer()
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true; layer = display; display.videoGravity = .resizeAspect
    }
    required init?(coder: NSCoder) { return nil }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override var acceptsFirstResponder: Bool { false }
}

final class NativeDecodeState: @unchecked Sendable {
    let lock = NSLock()
    var closed = false
    var error: UInt64 = 0
    var submitted: UInt64 = 0, decoded: UInt64 = 0, presented: UInt64 = 0, superseded: UInt64 = 0
    var inflight: UInt64 = 0
    var hardware = false
    var latest: (CVPixelBuffer, CMTime)?
    var lastQueued: CMTime?
    var scheduled = false
    var lastInput: UInt64?
    let width: Int, height: Int
    weak var view: VideoContainer?
    init(width: Int, height: Int) { self.width = width; self.height = height }
    func begin(_ pts: UInt64) -> Int32 {
        lock.lock(); defer { lock.unlock() }
        if closed || error != 0 || (lastInput.map { pts <= $0 } ?? false) { return -1 }
        if inflight >= 2 { return 0 }
        lastInput = pts; inflight += 1; submitted += 1; return 1
    }
    func rejected() { lock.lock(); if inflight > 0 { inflight -= 1 }; error = 3; lock.unlock() }
    func complete(_ status: OSStatus, _ flags: VTDecodeInfoFlags, _ image: CVImageBuffer?, _ time: CMTime) {
        lock.lock()
        if inflight > 0 { inflight -= 1 }
        if closed || error != 0 { lock.unlock(); return }
        guard status == noErr else { error = 3; lock.unlock(); return }
        guard let image = image else {
            if !flags.contains(.frameDropped) { error = 4 }
            lock.unlock(); return
        }
        guard CVPixelBufferGetWidth(image) == width && CVPixelBufferGetHeight(image) == height &&
                CVPixelBufferGetPixelFormatType(image) == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange &&
                time.flags.contains(.valid) && time.timescale > 0 else { error = 4; lock.unlock(); return }
        decoded += 1
        if let previous = lastQueued, CMTimeCompare(time, previous) <= 0 { superseded += 1; lock.unlock(); return }
        if latest != nil { superseded += 1 }
        latest = (image, time); lastQueued = time
        let wake = !scheduled; scheduled = true
        lock.unlock()
        if wake { DispatchQueue.main.async { self.present() } }
    }
    @discardableResult func observeDisplayStatus(_ status: AVQueuedSampleBufferRenderingStatus) -> Bool {
        lock.lock(); defer { lock.unlock() }
        if status == .failed { error = 5; latest = nil; scheduled = false }
        return closed || error != 0
    }
    @MainActor func resume() {
        lock.lock()
        let wake = !closed && error == 0 && latest != nil && !scheduled
        if wake { scheduled = true }
        lock.unlock()
        if wake { DispatchQueue.main.async { self.present() } }
    }
    @MainActor func present() {
        if let view = view, observeDisplayStatus(view.display.status) { return }
        lock.lock()
        guard !closed, error == 0, let frame = latest, let view = view else { scheduled = false; lock.unlock(); return }
        if !view.display.isReadyForMoreMediaData {
            let visible = view.window?.isVisible == true && view.window?.isMiniaturized == false
            if !visible { scheduled = false }
            lock.unlock()
            if visible { DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { self.present() } }
            return
        }
        latest = nil; scheduled = false; lock.unlock()
        var format: CMVideoFormatDescription?
        var sample: CMSampleBuffer?
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: .zero, decodeTimeStamp: .invalid)
        let formatted = CMVideoFormatDescriptionCreateForImageBuffer(allocator: kCFAllocatorDefault, imageBuffer: frame.0, formatDescriptionOut: &format)
        let ready = formatted == noErr && format != nil ? CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault, imageBuffer: frame.0, formatDescription: format!, sampleTiming: &timing,
            sampleBufferOut: &sample) : OSStatus(-1)
        guard ready == noErr, let sample = sample else { lock.lock(); error = 5; lock.unlock(); return }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true), CFArrayGetCount(attachments) == 1 {
            let entry = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(entry, Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        view.display.enqueue(sample)
        lock.lock(); presented += 1; if view.display.status == .failed { error = 5 }; lock.unlock()
    }
    func close() { lock.lock(); closed = true; latest = nil; lock.unlock() }
}

let nativeVideoDecoded: VTDecompressionOutputCallback = { pointer, _, status, flags, image, time, _ in
    guard let pointer = pointer else { return }
    Unmanaged<NativeDecodeState>.fromOpaque(pointer).takeUnretainedValue().complete(status, flags, image, time)
}

func nativeVideoFormat(_ bytes: [UInt8]) throws -> CMVideoFormatDescription {
    try videoCheck(bytes.count >= 7 && bytes.count <= 65536 && bytes[0] == 1 && bytes[1] == 66 && bytes[4] == 255 && bytes[5] == 225)
    var at = 6
    var parameters: [[UInt8]] = []
    for type in [7, 8] {
        if type == 8 { try videoCheck(at < bytes.count && bytes[at] == 1); at += 1 }
        try videoCheck(at + 2 <= bytes.count)
        let length = Int(bytes[at]) * 256 + Int(bytes[at + 1]); at += 2
        try videoCheck(length >= (type == 7 ? 4 : 1) && length <= bytes.count - at)
        let nal = Array(bytes[at..<at + length]); at += length
        try videoCheck(nal[0] & 128 == 0 && nal[0] & 31 == type)
        if type == 7 { try videoCheck(nal[1] == 66 && nal[2] == bytes[2] && nal[3] == bytes[3]) }
        parameters.append(nal)
    }
    try videoCheck(at == bytes.count)
    var format: CMFormatDescription?
    let status = parameters[0].withUnsafeBufferPointer { sps in
        parameters[1].withUnsafeBufferPointer { pps in
            var pointers = [sps.baseAddress!, pps.baseAddress!]
            var lengths = [sps.count, pps.count]
            return CMVideoFormatDescriptionCreateFromH264ParameterSets(allocator: kCFAllocatorDefault, parameterSetCount: 2,
                parameterSetPointers: &pointers, parameterSetSizes: &lengths, nalUnitHeaderLength: 4, formatDescriptionOut: &format)
        }
    }
    try videoCheck(status == noErr && format != nil); return format!
}

@MainActor final class NativeVideoSurface {
    let host: NSView
    let container: VideoContainer
    var state: NativeDecodeState?
    var decoder: VTDecompressionSession?
    var format: CMVideoFormatDescription?
    var closed = false
    init(host: NSView) {
        self.host = host; container = VideoContainer(frame: .zero)
        host.addSubview(container); container.isHidden = true
    }
    func configure(_ bytes: [UInt8], _ width: Int, _ height: Int) throws {
        try videoCheck(!closed && decoder == nil && width > 0 && height > 0 && width <= 1920 && height <= 1080 && width % 2 == 0 && height % 2 == 0)
        let format = try nativeVideoFormat(bytes)
        let dimensions = CMVideoFormatDescriptionGetDimensions(format)
        try videoCheck(dimensions.width == width && dimensions.height == height && DecoderBudget.shared.take())
        var release = true
        defer { if release { DecoderBudget.shared.release() } }
        let state = NativeDecodeState(width: width, height: height); state.view = container
        var callback = VTDecompressionOutputCallbackRecord(decompressionOutputCallback: nativeVideoDecoded,
            decompressionOutputRefCon: Unmanaged.passUnretained(state).toOpaque())
        let specification: [String: Any] = [kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder as String: true]
        let attributes: [String: Any] = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            kCVPixelBufferIOSurfacePropertiesKey as String: [:], kCVPixelBufferMetalCompatibilityKey as String: true]
        var session: VTDecompressionSession?
        let result = VTDecompressionSessionCreate(allocator: kCFAllocatorDefault, formatDescription: format,
            decoderSpecification: specification as CFDictionary, imageBufferAttributes: attributes as CFDictionary,
            outputCallback: &callback, decompressionSessionOut: &session)
        try videoCheck(result == noErr && session != nil)
        guard let session = session else { throw NativeVideoFailure.invalid }
        if VTSessionSetProperty(session, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue) != noErr {
            VTDecompressionSessionInvalidate(session); throw NativeVideoFailure.invalid
        }
        self.decoder = session; self.state = state; self.format = format; release = false
    }
    func submit(_ bytes: UnsafeBufferPointer<UInt8>, _ pts: UInt64, _ idr: Bool) -> Int32 {
        guard !closed, let state = state, let decoder = decoder, let format = format,
              !bytes.isEmpty, bytes.count <= 2 * 1024 * 1024, pts <= UInt64(Int64.max) else { return -1 }
        let accepted = state.begin(pts); if accepted != 1 { return accepted }
        var submitted = false
        defer { if !submitted { state.rejected() } }
        var block: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: bytes.count,
            blockAllocator: kCFAllocatorDefault, customBlockSource: nil, offsetToData: 0, dataLength: bytes.count,
            flags: 0, blockBufferOut: &block) == noErr, let block = block else { return -1 }
        let copied = CMBlockBufferReplaceDataBytes(with: bytes.baseAddress!, blockBuffer: block, offsetIntoDestination: 0, dataLength: bytes.count)
        guard copied == noErr else { return -1 }
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMTime(value: Int64(pts), timescale: 1000000), decodeTimeStamp: .invalid)
        var size = bytes.count
        var sample: CMSampleBuffer?
        guard CMSampleBufferCreateReady(allocator: kCFAllocatorDefault, dataBuffer: block, formatDescription: format,
            sampleCount: 1, sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1,
            sampleSizeArray: &size, sampleBufferOut: &sample) == noErr, let sample = sample else { return -1 }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true), CFArrayGetCount(attachments) == 1 {
            let entry = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(entry, Unmanaged.passUnretained(kCMSampleAttachmentKey_NotSync).toOpaque(),
                                 Unmanaged.passUnretained(idr ? kCFBooleanFalse : kCFBooleanTrue).toOpaque())
        }
        let result = VTDecompressionSessionDecodeFrame(decoder, sampleBuffer: sample, flags: [._EnableAsynchronousDecompression], frameRefcon: nil, infoFlagsOut: nil)
        if result != noErr { return -1 }; submitted = true; return 1
    }
    func place(_ x: Double, _ y: Double, _ width: Double, _ height: Double, _ visible: Bool) {
        guard !closed && [x,y,width,height].allSatisfy({ $0.isFinite }) && width > 0 && height > 0 else { container.isHidden = true; return }
        container.frame = NSRect(x: x, y: host.isFlipped ? y : host.bounds.height-y-height, width: width, height: height)
        container.display.frame = container.bounds; container.isHidden = !visible
        if visible { state?.resume() }
    }
    func stats() -> [UInt64] {
        guard let state = state else { return [0,0,0,0,0,0,closed ? 1 : 0,0] }
        if let decoder = decoder {
            state.lock.lock(); let query = state.decoded > 0 && !state.hardware && state.error == 0; state.lock.unlock()
            if query {
                var value: CFTypeRef?
                let result = VTSessionCopyProperty(decoder, key: kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder, allocator: kCFAllocatorDefault, valueOut: &value)
                state.lock.lock(); state.hardware = result == noErr && value as? Bool == true
                if !state.hardware { state.error = 6 }; state.lock.unlock()
            }
        }
        state.observeDisplayStatus(container.display.status)
        state.lock.lock(); defer { state.lock.unlock() }
        return [state.submitted,state.decoded,state.presented,state.superseded,state.inflight,state.hardware ? 1 : 0,state.error,container.display.status == .rendering ? 1 : 0]
    }
    func close() {
        if closed { return }; closed = true
        state?.close(); container.display.flushAndRemoveImage(); container.removeFromSuperview()
        let retainedState = state; let session = decoder; decoder = nil; format = nil
        if let session = session {
            DispatchQueue.global(qos: .userInitiated).async {
                withExtendedLifetime(retainedState) {
                    VTDecompressionSessionWaitForAsynchronousFrames(session)
                    VTDecompressionSessionInvalidate(session)
                }
                DecoderBudget.shared.release()
            }
        }
    }
}

@MainActor @_cdecl("freelane_video_create")
func freelaneVideoCreate(_ view: UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer? {
    guard Thread.isMainThread, let view = view else { return nil }
    return Unmanaged.passRetained(NativeVideoSurface(host: Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue())).toOpaque()
}
@MainActor @_cdecl("freelane_video_configure")
func freelaneVideoConfigure(_ context: UnsafeMutableRawPointer?, _ data: UnsafePointer<UInt8>?, _ length: Int, _ width: UInt32, _ height: UInt32) -> Int32 {
    guard let context = context, let data = data, length > 0 && length <= 65536 else { return -1 }
    do { try Unmanaged<NativeVideoSurface>.fromOpaque(context).takeUnretainedValue().configure(Array(UnsafeBufferPointer(start: data,count: length)),Int(width),Int(height)); return 1 }
    catch { return -1 }
}
@MainActor @_cdecl("freelane_video_submit")
func freelaneVideoSubmit(_ context: UnsafeMutableRawPointer?, _ data: UnsafePointer<UInt8>?, _ length: Int, _ pts: UInt64, _ idr: Int32) -> Int32 {
    guard let context = context, let data = data, length > 0 && length <= 2*1024*1024 && (idr == 0 || idr == 1) else { return -1 }
    return Unmanaged<NativeVideoSurface>.fromOpaque(context).takeUnretainedValue().submit(UnsafeBufferPointer(start: data,count: length),pts,idr == 1)
}
@MainActor @_cdecl("freelane_video_layout")
func freelaneVideoLayout(_ context: UnsafeMutableRawPointer?, _ x: Double, _ y: Double, _ width: Double, _ height: Double, _ visible: Int32) {
    if let context = context { Unmanaged<NativeVideoSurface>.fromOpaque(context).takeUnretainedValue().place(x,y,width,height,visible != 0) }
}
@MainActor @_cdecl("freelane_video_stats")
func freelaneVideoStats(_ context: UnsafeMutableRawPointer?, _ output: UnsafeMutablePointer<UInt64>?) {
    guard let context = context, let output = output else { return }
    let values = Unmanaged<NativeVideoSurface>.fromOpaque(context).takeUnretainedValue().stats()
    for index in 0..<8 { output[index] = values[index] }
}
@MainActor @_cdecl("freelane_video_destroy")
func freelaneVideoDestroy(_ context: UnsafeMutableRawPointer?) {
    if let context = context { Unmanaged<NativeVideoSurface>.fromOpaque(context).takeRetainedValue().close() }
}
