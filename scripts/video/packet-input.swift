import Foundation
import AVFoundation

struct CompressedFixture {
    let samples: [CMSampleBuffer]
    let leading: Int
    let trailing: Int
    let raw: Bool
}
@MainActor func loadFixture(_ url: URL) throws -> CompressedFixture {
    if url.pathExtension == "flv" { return try loadRawPackets(url) }
    let asset = AVURLAsset(url: url)
    guard let track = asset.tracks(withMediaType: .video).first else { throw ProbeFailure(message: "No video track") }
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
    output.alwaysCopiesSampleData = false
    try require(reader.canAdd(output), "Compressed reader unavailable")
    reader.add(output)
    try require(reader.startReading(), "Cannot read compressed fixture")
    var samples: [CMSampleBuffer] = []
    var emptyContainerBuffers = 0
    var trailingEmptyContainerBuffers = 0
    while let sample = output.copyNextSampleBuffer() {
        let bytes = CMSampleBufferGetTotalSampleSize(sample)
        let count = CMSampleBufferGetNumSamples(sample)
        let pts = CMSampleBufferGetPresentationTimeStamp(sample)
        // AVAssetReader exposes the MP4's leading empty edit as a zero-byte buffer.
        // It contains no coded picture/reference; accept at most one leading empty marker.
        if bytes == 0 && samples.isEmpty && emptyContainerBuffers == 0 {
            emptyContainerBuffers += 1
            continue
        }
        // The reader also emits non-picture end/duration markers with no samples/data.
        // Drain at most 64 non-picture markers, then require actual reader completion.
        // Any subsequent coded picture still fails the fixed 30-picture bound.
        if bytes == 0 && count == 0 && samples.count == 30 && trailingEmptyContainerBuffers < 64 {
            if pts.isValid {
                let lastPictureTime = CMSampleBufferGetPresentationTimeStamp(samples[29])
                try require(pts.timescale > 0 && CMTimeCompare(pts, lastPictureTime) >= 0 && CMTimeCompare(pts, CMTime(value: 1, timescale: 1)) <= 0,
                            "Unexpected terminal marker time")
            }
            trailingEmptyContainerBuffers += 1
            continue
        }
        try require(trailingEmptyContainerBuffers == 0 && samples.count < 30 && bytes > 0 && bytes <= 2 * 1024 * 1024 && count == 1 && pts.isValid && pts.timescale > 0,
                    "Compressed picture: buffers=\(samples.count), bytes=\(bytes), count=\(count), validPTS=\(pts.isValid), pts=\(pts.value)/\(pts.timescale), emptyMarkers=\(emptyContainerBuffers), terminalMarkers=\(trailingEmptyContainerBuffers), reader=\(reader.status.rawValue)")
        if samples.isEmpty { try require(CMTimeCompare(pts, .zero) == 0, "First coded picture must start at zero") }
        if let previous = samples.last {
            try require(CMTimeCompare(pts, CMSampleBufferGetPresentationTimeStamp(previous)) > 0, "Compressed timestamps did not advance")
        }
        samples.append(sample)
    }
    try require(reader.status == .completed && samples.count == 30, "Expected30 compressed frames: count=\(samples.count), reader=\(reader.status.rawValue), terminalMarkers=\(trailingEmptyContainerBuffers), readerError=\(reader.error.map { ($0 as NSError).code } ?? 0)")
    return CompressedFixture(samples: samples, leading: emptyContainerBuffers, trailing: trailingEmptyContainerBuffers, raw: false)
}

func rawFormat(_ bytes: [UInt8]) throws -> CMVideoFormatDescription {
    try require(bytes.count >= 7 && bytes[0] == 1 && bytes[1] == 66 && bytes[4] == 255 && bytes[5] == 225, "Invalid raw AVC configuration")
    var at = 6
    var parameters: [[UInt8]] = []
    for type in [7, 8] {
        if type == 8 { try require(at < bytes.count && bytes[at] == 1, "Expected one raw PPS"); at += 1 }
        try require(at + 2 <= bytes.count, "Truncated raw parameter length")
        let length = Int(bytes[at]) * 256 + Int(bytes[at + 1]); at += 2
        try require(length >= (type == 7 ? 4 : 1) && length <= bytes.count - at, "Invalid raw parameter size")
        let nal = Array(bytes[at..<at + length]); at += length
        try require(nal[0] & 128 == 0 && nal[0] & 31 == type && (type != 7 || nal[1] == 66), "Invalid raw parameter type/profile")
        parameters.append(nal)
    }
    try require(at == bytes.count, "Trailing raw configuration")
    var format: CMFormatDescription?
    let status = parameters[0].withUnsafeBufferPointer { sps in
        parameters[1].withUnsafeBufferPointer { pps in
            var pointers = [sps.baseAddress!, pps.baseAddress!]
            var sizes = [sps.count, pps.count]
            return CMVideoFormatDescriptionCreateFromH264ParameterSets(allocator: kCFAllocatorDefault, parameterSetCount: 2, parameterSetPointers: &pointers, parameterSetSizes: &sizes, nalUnitHeaderLength: 4, formatDescriptionOut: &format)
        }
    }
    try require(status == noErr && format != nil, "Native raw H264 format failed: \(status)")
    return format!
}
func rawSample(_ bytes: [UInt8], format: CMVideoFormatDescription, pts: UInt64) throws -> CMSampleBuffer {
    var block: CMBlockBuffer?
    let created = CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: bytes.count, blockAllocator: kCFAllocatorDefault, customBlockSource: nil, offsetToData: 0, dataLength: bytes.count, flags: 0, blockBufferOut: &block)
    try require(created == noErr && block != nil, "Native raw block allocation failed: \(created)")
    let copied = bytes.withUnsafeBytes { pointer in
        CMBlockBufferReplaceDataBytes(with: pointer.baseAddress!, blockBuffer: block!, offsetIntoDestination: 0, dataLength: bytes.count)
    }
    try require(copied == noErr, "Native raw block copy failed: \(copied)")
    var sample: CMSampleBuffer?
    var timing = CMSampleTimingInfo(duration: CMTime(value: 33333, timescale: 1000000), presentationTimeStamp: CMTime(value: Int64(pts), timescale: 1000000), decodeTimeStamp: .invalid)
    var size = bytes.count
    let ready = CMSampleBufferCreateReady(allocator: kCFAllocatorDefault, dataBuffer: block, formatDescription: format, sampleCount: 1, sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1, sampleSizeArray: &size, sampleBufferOut: &sample)
    try require(ready == noErr && sample != nil, "Native raw sample creation failed: \(ready)")
    return sample!
}
func loadRawPackets(_ url: URL) throws -> CompressedFixture {
    let file = try FileHandle(forReadingFrom: url)
    defer { try? file.close() }
    let bytes = [UInt8](try file.read(upToCount: 4 * 1024 * 1024 + 1) ?? Data())
    try require(!bytes.isEmpty && bytes.count <= 4 * 1024 * 1024, "Raw fixture exceeds bounds")
    var at = 0
    var records = 0
    var binding: [UInt8]?
    var format: CMVideoFormatDescription?
    var samples: [CMSampleBuffer] = []
    var lastPTS: UInt64?
    func integer(_ start: Int, _ length: Int) -> UInt64 { bytes[start..<start + length].reduce(0) { ($0 << 8) | UInt64($1) } }
    while at < bytes.count {
        try require(records < 31 && bytes.count - at >= 64, "Raw record count/header invalid")
        let h = at
        try require(Array(bytes[h..<h + 4]) == [70, 76, 86, 49] && bytes[h + 6] == 66 && bytes[h + 7] == 0, "Unsupported raw protocol/profile")
        try require(integer(h + 12, 2) == 320 && integer(h + 14, 2) == 180, "Raw fixture dimensions invalid")
        let identity = Array(bytes[h + 16..<h + 48])
        try require(bytes[h + 16..<h + 32].contains { $0 != 0 } && integer(h + 32, 8) > 0 && integer(h + 40, 8) > 0, "Invalid raw session")
        if let expected = binding { try require(identity == expected, "Raw session changed") } else { binding = identity }
        let kind = bytes[h + 4], flag = bytes[h + 5]
        let length = Int(integer(h + 8, 4)), sequence = integer(h + 48, 8), pts = integer(h + 56, 8)
        try require(length > 0 && length <= (kind == 1 ? 65536 : 2 * 1024 * 1024) && length <= bytes.count - at - 64, "Raw payload exceeds bounds")
        at += 64
        let payload = Array(bytes[at..<at + length]); at += length
        if kind == 1 {
            try require(records == 0 && flag == 0 && sequence == 0 && pts == 0 && format == nil, "Unexpected raw configuration")
            format = try rawFormat(payload)
        } else {
            guard let format = format else { throw ProbeFailure(message: "Raw frame before configuration") }
            try require(kind == 2 && flag <= 1 && sequence == UInt64(samples.count + 1) && pts <= 1000000, "Raw picture header invalid")
            if let previous = lastPTS { try require(pts > previous, "Raw time regressed") } else { try require(flag == 1 && pts == 0, "Raw first frame must be IDR at zero") }
            var offset = 0, nals = 0
            var idr = false, dependent = false
            while offset < payload.count {
                try require(payload.count - offset >= 4 && nals < 256, "Raw NAL header invalid")
                let size = payload[offset..<offset + 4].reduce(0) { ($0 << 8) | Int($1) }; offset += 4
                try require(size > 0 && size <= payload.count - offset && payload[offset] & 128 == 0, "Raw NAL length invalid")
                let type = payload[offset] & 31
                try require([1, 5, 6, 9, 12].contains(type), "Raw NAL type invalid")
                if type == 5 { idr = true }; if type == 1 { dependent = true }
                offset += size; nals += 1
            }
            try require((idr || dependent) && !(idr && dependent) && idr == (flag == 1), "Raw IDR flag mismatch")
            samples.append(try rawSample(payload, format: format, pts: pts)); lastPTS = pts
        }
        records += 1
    }
    try require(records == 31 && samples.count == 30, "Expected30 direct compressed pictures")
    return CompressedFixture(samples: samples, leading: 0, trailing: 0, raw: true)
}
