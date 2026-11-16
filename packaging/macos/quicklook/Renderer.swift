// OpenStitch Quick Look thumbnails (#557, #1182): calls the Rust renderer linked into the
// extension (render/, a static library; openstitch_quicklook.h) with the file's bytes and gets
// the PNG back. It runs in-process: Quick Look's sandbox for thumbnail extensions refuses to
// start any process, so the renderer cannot be a helper program. No drawing or file-format
// code lives in Swift; the Rust crate openstitch-render does all of it.
import Foundation

enum ThumbnailError: Error, CustomStringConvertible {
    case timedOut
    case failed(String)
    case notAPicture

    var description: String {
        switch self {
        case .timedOut: return "the OpenStitch renderer took too long"
        case let .failed(message): return "the OpenStitch renderer failed: \(message)"
        case .notAPicture: return "the OpenStitch renderer gave no PNG"
        }
    }
}

enum Renderer {
    /// Largest file handed to the renderer, as openstitch_render::thumbnail::MAX_INPUT.
    static let maxInput = 64 << 20
    /// The thumbnail is given up after this long (a malformed file must never hang Finder).
    static let timeout: TimeInterval = 10

    /// The PNG thumbnail (`size` x `size` pixels) of a design file's `data`, its format named
    /// by `fileExtension` (pes, dst, osd, ...).
    ///
    /// The renderer runs on its own thread and the caller waits at most `timeout`. Code in the
    /// same process cannot be stopped, so on a timeout that thread finishes on its own (and
    /// frees its result) while the caller already has its error.
    static func png(data: Data, fileExtension: String, size: Int) throws -> Data {
        if data.count > maxInput {
            throw ThumbnailError.failed("larger than 64 MiB")
        }
        let pixels = UInt32(min(max(size, 16), 1024))
        let result = Result()
        let done = DispatchSemaphore(value: 0)
        DispatchQueue.global(qos: .userInitiated).async {
            result.set(render(data: data, fileExtension: fileExtension, size: pixels))
            done.signal()
        }
        guard done.wait(timeout: .now() + timeout) == .success, let outcome = result.value else {
            throw ThumbnailError.timedOut
        }
        let png = try outcome.get()
        guard png.starts(with: [0x89, 0x50, 0x4E, 0x47]) else {
            throw ThumbnailError.notAPicture
        }
        return png
    }

    private static func render(data: Data, fileExtension: String, size: UInt32)
        -> Swift.Result<Data, ThumbnailError>
    {
        var buffer = OpenStitchBuffer(ptr: nil, len: 0)
        let code = data.withUnsafeBytes { bytes in
            fileExtension.withCString { ext in
                openstitch_ql_thumbnail(
                    bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, ext, size, &buffer)
            }
        }
        defer { openstitch_ql_free(buffer) }
        let out = buffer.ptr.map { Data(bytes: $0, count: buffer.len) } ?? Data()
        return code == 0
            ? .success(out) : .failure(.failed(String(decoding: out, as: UTF8.self)))
    }
}

/// The renderer's outcome, handed from its thread to the waiting caller.
private final class Result: @unchecked Sendable {
    private let lock = NSLock()
    private var outcome: Swift.Result<Data, ThumbnailError>?
    func set(_ value: Swift.Result<Data, ThumbnailError>) {
        lock.lock(); outcome = value; lock.unlock()
    }
    var value: Swift.Result<Data, ThumbnailError>? {
        lock.lock(); defer { lock.unlock() }; return outcome
    }
}
