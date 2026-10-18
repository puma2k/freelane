// Experimental native decode/surface probe. Input is a generated Windows SDK fixture.
import Foundation
import AVFoundation
import VideoToolbox
import AppKit
import CoreVideo

struct ProbeFailure: Error { let message: String }
func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw ProbeFailure(message: message) }
}
final class DecodeState: @unchecked Sendable {
    let lock = NSLock()
    let slots = DispatchSemaphore(value: 2)
    var frames = 0
    var valid = true
    var shades = Set<UInt8>()
}
let callback: VTDecompressionOutputCallback = { context, _, status, _, image, _, _ in
    guard let context = context else { return }
    let state = Unmanaged<DecodeState>.fromOpaque(context).takeUnretainedValue()
    defer { state.slots.signal() }
    var valid = status == noErr
    var shade: UInt8 = 0
    if let image = image {
        valid = valid && CVPixelBufferGetWidth(image) == 320 && CVPixelBufferGetHeight(image) == 180
        valid = valid && CVPixelBufferGetPixelFormatType(image) == kCVPixelFormatType_32BGRA
        if valid && CVPixelBufferLockBaseAddress(image, .readOnly) == kCVReturnSuccess {
            if let bytes = CVPixelBufferGetBaseAddress(image) {
                let offset = 90 * CVPixelBufferGetBytesPerRow(image) + 160 * 4
                let b = bytes.load(fromByteOffset: offset, as: UInt8.self)
                let g = bytes.load(fromByteOffset: offset + 1, as: UInt8.self)
                let r = bytes.load(fromByteOffset: offset + 2, as: UInt8.self)
                shade = g
                valid = g > 20 && g < 240 && abs(Int(r)-Int(g)) < 15 && abs(Int(b)-Int(g)) < 15
            } else { valid = false }
            CVPixelBufferUnlockBaseAddress(image, .readOnly)
        } else { valid = false }
    } else { valid = false }
    state.lock.lock()
    state.frames += 1
    state.valid = state.valid && valid
    state.shades.insert(shade)
    state.lock.unlock()
}
@MainActor func probe(_ url: URL, failAfterFirstSubmission: Bool = false) throws -> [String: Any] {
    let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
    try require(attributes[.type] as? FileAttributeType == .typeRegular, "Fixture must be a regular file")
    let length = (attributes[.size] as? NSNumber)?.intValue ?? 0
    try require(length > 0 && length <= 4 * 1024 * 1024, "Fixture size exceeds probe bounds")
    let input = try loadFixture(url)
    let samples = input.samples
    guard let format = CMSampleBufferGetFormatDescription(samples[0]) else { throw ProbeFailure(message: "No H.264 format") }
    let dimensions = CMVideoFormatDescriptionGetDimensions(format)
    try require(CMFormatDescriptionGetMediaSubType(format) == kCMVideoCodecType_H264 && dimensions.width == 320 && dimensions.height == 180, "Unexpected codec/dimensions")
    let state = DecodeState()
    var record = VTDecompressionOutputCallbackRecord(decompressionOutputCallback: callback, decompressionOutputRefCon: Unmanaged.passUnretained(state).toOpaque())
    var decoder: VTDecompressionSession?
    let specification: [String: Any] = [kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder as String: true]
    let pixels: [String: Any] = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA, kCVPixelBufferMetalCompatibilityKey as String: true]
    try require(VTDecompressionSessionCreate(allocator: kCFAllocatorDefault, formatDescription: format, decoderSpecification: specification as CFDictionary, imageBufferAttributes: pixels as CFDictionary, outputCallback: &record, decompressionSessionOut: &decoder) == noErr, "Cannot create decompression session")
    guard let decoder = decoder else { throw ProbeFailure(message: "No decoder") }
    defer {
        withExtendedLifetime(state) {
            VTDecompressionSessionWaitForAsynchronousFrames(decoder)
            VTDecompressionSessionInvalidate(decoder)
        }
    }
    let realtime = VTSessionSetProperty(decoder, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue) == noErr
    let started = CFAbsoluteTimeGetCurrent()
    for sample in samples {
        try require(state.slots.wait(timeout: .now() + 5) == .success, "Decoder output stalled")
        let status = VTDecompressionSessionDecodeFrame(decoder, sampleBuffer: sample, flags: [._EnableAsynchronousDecompression], frameRefcon: nil, infoFlagsOut: nil)
        if status != noErr { state.slots.signal(); throw ProbeFailure(message: "Decoder rejected compressed sample") }
        if failAfterFirstSubmission { throw ProbeFailure(message: "Injected failure after asynchronous submission") }
    }
    try require(VTDecompressionSessionWaitForAsynchronousFrames(decoder) == noErr, "Decoder did not finish")
    let decodeMilliseconds = (CFAbsoluteTimeGetCurrent() - started) * 1000
    var hardwareValue: CFTypeRef?
    let hardwareReadback = VTSessionCopyProperty(decoder, key: kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder, allocator: kCFAllocatorDefault, valueOut: &hardwareValue) == noErr
    let hardware = hardwareReadback && (hardwareValue as? Bool == true)
    state.lock.lock()
    let count = state.frames, valid = state.valid, varying = state.shades.count >= 2
    state.lock.unlock()
    try require(count == 30 && valid && varying, "Decoded pixels/frame count do not match generated fixture")

    let application = NSApplication.shared
    application.setActivationPolicy(.accessory)
    application.finishLaunching()
    let window = NSWindow(contentRect: NSRect(x: 40, y: 40, width: 400, height: 260), styleMask: [.titled, .closable], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.title = "FreeLane owned video probe"
    let layer = AVSampleBufferDisplayLayer()
    layer.videoGravity = .resizeAspect
    guard let view = window.contentView else { throw ProbeFailure(message: "No owned native surface") }
    view.wantsLayer = true
    view.layer = layer
    layer.frame = view.bounds
    window.makeKeyAndOrderFront(nil)
    defer { layer.flushAndRemoveImage(); window.close() }
    var timebase: CMTimebase?
    try require(CMTimebaseCreateWithSourceClock(allocator: kCFAllocatorDefault, sourceClock: CMClockGetHostTimeClock(), timebaseOut: &timebase) == noErr, "Cannot create surface timebase")
    if let timebase = timebase {
        layer.controlTimebase = timebase
        CMTimebaseSetTime(timebase, time: CMSampleBufferGetPresentationTimeStamp(samples[0]))
        CMTimebaseSetRate(timebase, rate: 1)
    }
    let deadline = Date().addingTimeInterval(5)
    for sample in samples {
        while !layer.isReadyForMoreMediaData && Date() < deadline && layer.status != .failed {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
        try require(layer.isReadyForMoreMediaData && layer.status != .failed, "Native surface rejected samples")
        layer.enqueue(sample)
    }
    RunLoop.main.run(until: Date().addingTimeInterval(1.2))
    let rendering = layer.status == .rendering && layer.error == nil
    #if arch(arm64)
    let architecture = "arm64"
    #else
    let architecture = "x86_64"
    #endif
    return ["schema": 1, "platform": "macos", "architecture": architecture,
            "decoded_frames": count, "direct_packets_verified": input.raw, "leading_empty_container_buffers": input.leading,
            "trailing_empty_container_buffers": input.trailing, "dimensions_verified": valid,
            "pixel_variation_verified": varying, "hardware_decoder_used": hardware,
            "hardware_readback_available": hardwareReadback, "realtime_property_applied": realtime,
            "owned_surface_created": true, "surface_rendering": rendering,
            "onscreen_pixels_verified": false, "copy_path_measured": false,
            "validation_cpu_readback": true, "decode_file_elapsed_ms": decodeMilliseconds,
            "continuous_video_accepted": false]
}
@main struct ProbeMain {
    @MainActor static func main() {
        do {
            let arguments = CommandLine.arguments
            let injectedFailure = arguments.count == 3 && arguments[2] == "--fail-after-first-submit"
            try require(arguments.count == 2 || injectedFailure, "Usage: probe-macos GENERATED_FIXTURE_MP4 [--fail-after-first-submit]")
            let result = try probe(URL(fileURLWithPath: arguments[1]), failAfterFirstSubmission: injectedFailure)
            let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
            FileHandle.standardOutput.write(data); FileHandle.standardOutput.write(Data([10]))
        } catch {
            let message = (error as? ProbeFailure)?.message ?? "Native video probe failed"
            FileHandle.standardError.write(Data((message + "\n").utf8))
            exit(1)
        }
    }
}
