import AVFoundation
import CoreGraphics
import Foundation
import ScreenCaptureKit

// A small native helper used by the macOS backend. ScreenCaptureKit captures
// the display while explicitly excluding this application's windows, which
// keeps the visible recording controls out of the resulting movie.
@available(macOS 12.3, *)
final class Recorder: NSObject, SCStreamOutput, SCStreamDelegate {
    private let outputURL: URL
    private let width: Int
    private let height: Int
    private let completed = DispatchSemaphore(value: 0)
    private let queue = DispatchQueue(label: "com.taichi.screenshot.sck.writer")
    private var writer: AVAssetWriter?
    private var videoInput: AVAssetWriterInput?
    private var stream: SCStream?
    private var writerStarted = false
    private var terminalError: Error?

    init(outputURL: URL, width: Int, height: Int) {
        self.outputURL = outputURL
        self.width = width
        self.height = height
    }

    func start(display: SCDisplay, sourceRect: CGRect, excludedApplication: SCRunningApplication?) async throws {
        let config = SCStreamConfiguration()
        config.sourceRect = sourceRect
        config.width = width
        config.height = height
        config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
        config.queueDepth = 5
        config.pixelFormat = kCVPixelFormatType_32BGRA

        let filter: SCContentFilter
        if let excludedApplication {
            filter = SCContentFilter(display: display, excludingApplications: [excludedApplication], exceptingWindows: [])
        } else {
            filter = SCContentFilter(display: display, excludingWindows: [])
        }

        let writer = try AVAssetWriter(outputURL: outputURL, fileType: .mp4)
        let input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: width,
                AVVideoHeightKey: height,
                AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: max(width * height * 6, 2_000_000)],
            ]
        )
        input.expectsMediaDataInRealTime = true
        guard writer.canAdd(input) else { throw RecorderError.cannotAddVideoTrack }
        writer.add(input)
        self.writer = writer
        self.videoInput = input

        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
        self.stream = stream
        try await stream.startCapture()
        fputs("[sck] ready output=\(outputURL.path) size=\(width)x\(height)\n", stderr)
    }

    func stop() {
        Task {
            try? await stream?.stopCapture()
            finish(nil)
        }
    }

    func wait() -> Error? {
        completed.wait()
        return terminalError
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of outputType: SCStreamOutputType) {
        guard outputType == .screen, CMSampleBufferDataIsReady(sampleBuffer) else { return }
        queue.async { [weak self] in
            guard let self, let writer = self.writer, let input = self.videoInput else { return }
            if !self.writerStarted {
                guard writer.startWriting() else {
                    self.finish(writer.error ?? RecorderError.cannotStartWriter)
                    return
                }
                writer.startSession(atSourceTime: CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
                self.writerStarted = true
            }
            if input.isReadyForMoreMediaData {
                _ = input.append(sampleBuffer)
            }
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        finish(error)
    }

    private func finish(_ error: Error?) {
        queue.async { [weak self] in
            guard let self, self.terminalError == nil else { return }
            self.terminalError = error
            guard let writer = self.writer, let input = self.videoInput, self.writerStarted else {
                self.completed.signal()
                return
            }
            input.markAsFinished()
            writer.finishWriting { self.completed.signal() }
        }
    }
}

enum RecorderError: Error { case cannotAddVideoTrack, cannotStartWriter, invalidArguments, displayNotFound }

@available(macOS 12.3, *)
func run() async throws {
    let args = Array(CommandLine.arguments.dropFirst())
    guard args.count == 6,
          let displayIndex = Int(args[0]),
          let x = Double(args[1]), let y = Double(args[2]),
          let logicalWidth = Double(args[3]), let logicalHeight = Double(args[4]) else {
        throw RecorderError.invalidArguments
    }
    let outputURL = URL(fileURLWithPath: args[5])
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard content.displays.indices.contains(displayIndex) else { throw RecorderError.displayNotFound }
    let display = content.displays[displayIndex]
    let scale = CGFloat(display.width) / max(CGDisplayBounds(display.displayID).width, 1)
    let pixelWidth = max(2, Int((logicalWidth * Double(scale)).rounded()))
    let pixelHeight = max(2, Int((logicalHeight * Double(scale)).rounded()))
    let ownProcess = Int32(ProcessInfo.processInfo.processIdentifier)
    let ownApplication = content.applications.first { $0.processID == ownProcess }
    let recorder = Recorder(outputURL: outputURL, width: pixelWidth, height: pixelHeight)

    try await recorder.start(
        display: display,
        sourceRect: CGRect(x: x, y: y, width: logicalWidth, height: logicalHeight),
        excludedApplication: ownApplication
    )

    signal(SIGINT, SIG_IGN)
    // The main thread waits for completion below, so the signal handler must
    // run on an independent queue rather than the main dispatch queue.
    let signalSource = DispatchSource.makeSignalSource(signal: SIGINT, queue: .global(qos: .userInitiated))
    signalSource.setEventHandler {
        recorder.stop()
    }
    signalSource.resume()
    if let error = recorder.wait() { throw error }
    fputs("[sck] finished output=\(outputURL.path)\n", stderr)
}

if #available(macOS 12.3, *) {
    Task.detached {
        do {
            try await run()
            exit(0)
        } catch {
            fputs("[sck] failed: \(error)\n", stderr)
            exit(1)
        }
    }
    // ScreenCaptureKit delivers stream callbacks through the application's run
    // loop. Do not block the main thread while the recorder is active.
    RunLoop.main.run()
} else {
    fputs("[sck] requires macOS 12.3 or later\n", stderr)
    exit(1)
}
