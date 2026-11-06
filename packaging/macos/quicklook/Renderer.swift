// OpenStitch Quick Look thumbnails (#557): runs the Rust renderer, `openstitch-thumbnailer
// --stdin EXT`, with the file's bytes on stdin and reads the PNG from stdout. No drawing or
// file-format code lives in Swift; the Rust crate openstitch-render does all of it.
import Foundation

enum ThumbnailError: Error, CustomStringConvertible {
    case helperMissing
    case timedOut
    case killed(Int32)
    case failed(Int32, String)
    case notAPicture

    var description: String {
        switch self {
        case .helperMissing: return "openstitch-thumbnailer is missing from the extension"
        case .timedOut: return "openstitch-thumbnailer took too long"
        case let .killed(signal): return "openstitch-thumbnailer was stopped by signal \(signal)"
        case let .failed(code, message): return "openstitch-thumbnailer exited \(code): \(message)"
        case .notAPicture: return "openstitch-thumbnailer wrote no PNG"
        }
    }
}

enum Renderer {
    /// Largest file handed to the helper, as openstitch_render::thumbnail::MAX_INPUT.
    static let maxInput = 64 << 20
    /// The helper is stopped after this long (a malformed file must never hang Finder).
    static let timeout: TimeInterval = 10

    /// The PNG thumbnail (`size` x `size` pixels) of a design file's `data`, its format named
    /// by `fileExtension` (pes, dst, osd, ...).
    static func png(data: Data, fileExtension: String, size: Int, helper: URL) throws -> Data {
        guard FileManager.default.isExecutableFile(atPath: helper.path) else {
            throw ThumbnailError.helperMissing
        }
        if data.count > maxInput {
            throw ThumbnailError.failed(1, "larger than 64 MiB")
        }
        let process = Process()
        process.executableURL = helper
        process.arguments = ["-s", String(min(max(size, 16), 1024)), "--stdin", fileExtension]
        let input = Pipe()
        let output = Pipe()
        let errors = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        try process.run()

        // Feed stdin on its own queue so a full stdout pipe can never block the write.
        let writer = DispatchQueue(label: "openstitch.thumbnail.stdin")
        writer.async {
            try? input.fileHandleForWriting.write(contentsOf: data)
            try? input.fileHandleForWriting.close()
        }
        let stopped = Stopped()
        let killer = DispatchWorkItem {
            if process.isRunning {
                stopped.set()
                process.terminate()
            }
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + timeout, execute: killer)

        var png = Data()
        var message = Data()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global().async {
            message = errors.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        png = output.fileHandleForReading.readDataToEndOfFile()
        group.wait()
        process.waitUntilExit()
        killer.cancel()

        if process.terminationReason == .uncaughtSignal {
            throw stopped.value
                ? ThumbnailError.timedOut : ThumbnailError.killed(process.terminationStatus)
        }
        guard process.terminationStatus == 0 else {
            let text = String(decoding: message, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw ThumbnailError.failed(process.terminationStatus, text)
        }
        guard png.starts(with: [0x89, 0x50, 0x4E, 0x47]) else {
            throw ThumbnailError.notAPicture
        }
        return png
    }
}

/// Whether the timeout stopped the helper (set on another queue).
private final class Stopped: @unchecked Sendable {
    private let lock = NSLock()
    private var flag = false
    func set() { lock.lock(); flag = true; lock.unlock() }
    var value: Bool { lock.lock(); defer { lock.unlock() }; return flag }
}
