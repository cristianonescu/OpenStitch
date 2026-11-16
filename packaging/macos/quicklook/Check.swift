// `openstitch-quicklook-check`, for packaging/macos/quicklook/check.sh (#557, #1182):
//   openstitch-quicklook-check INPUT OUTPUT.png [SIZE]
//       Renderer.swift, the extension's own code path to the linked Rust renderer, outside
//       Quick Look.
//   openstitch-quicklook-check --system SECONDS INPUT OUTPUT.png [SIZE]
//       Asks macOS for the file's thumbnail as Finder does (QLThumbnailGenerator), through the
//       registered extension, and waits at most SECONDS. Exit 0 with the PNG written; 1 when
//       macOS gives no thumbnail or only the type's icon; 3 on the time limit (a hang).
import AppKit
import Foundation
import QuickLookThumbnailing

@main
struct Check {
    static func fail(_ message: String, _ code: Int32 = 1) -> Never {
        FileHandle.standardError.write(Data("openstitch-quicklook-check: \(message)\n".utf8))
        exit(code)
    }

    static func main() {
        var args = Array(CommandLine.arguments.dropFirst())
        if args.first == "--system" {
            guard args.count >= 4, let seconds = Double(args[1]) else { usage() }
            args.removeFirst(2)
            system(seconds: seconds, args: args)
        }
        guard args.count >= 2 else { usage() }
        do {
            let input = URL(fileURLWithPath: args[0])
            let png = try Renderer.png(
                data: try Data(contentsOf: input), fileExtension: input.pathExtension,
                size: args.count > 2 ? Int(args[2]) ?? 256 : 256)
            try png.write(to: URL(fileURLWithPath: args[1]))
            print("wrote \(args[1]) (\(png.count) bytes)")
        } catch {
            fail("\(error)")
        }
    }

    static func usage() -> Never {
        fail(
            "usage: openstitch-quicklook-check INPUT OUTPUT.png [SIZE]\n"
                + "       openstitch-quicklook-check --system SECONDS INPUT OUTPUT.png [SIZE]", 64)
    }

    static func system(seconds: Double, args: [String]) -> Never {
        let size = CGFloat(args.count > 2 ? Double(args[2]) ?? 256 : 256)
        let request = QLThumbnailGenerator.Request(
            fileAt: URL(fileURLWithPath: args[0]), size: CGSize(width: size, height: size),
            scale: 1, representationTypes: .thumbnail)
        let started = Date()
        let done = DispatchSemaphore(value: 0)
        var outcome: Result<CGImage, Error>?
        QLThumbnailGenerator.shared.generateBestRepresentation(for: request) { rep, error in
            if let rep = rep {
                outcome = .success(rep.cgImage)
            } else {
                outcome = .failure(error ?? ThumbnailError.notAPicture)
            }
            done.signal()
        }
        guard done.wait(timeout: .now() + seconds) == .success, let result = outcome else {
            QLThumbnailGenerator.shared.cancel(request)
            fail("no answer from Quick Look within \(seconds) s (a hang)", 3)
        }
        let elapsed = String(format: "%.2f", Date().timeIntervalSince(started))
        switch result {
        case let .failure(error):
            fail("Quick Look gave no thumbnail after \(elapsed) s: \(error)")
        case let .success(image):
            let rep = NSBitmapImageRep(cgImage: image)
            guard let png = rep.representation(using: .png, properties: [:]) else {
                fail("cannot encode the thumbnail")
            }
            do { try png.write(to: URL(fileURLWithPath: args[1])) } catch { fail("\(error)") }
            print("Quick Look drew \(image.width)x\(image.height) in \(elapsed) s: \(args[1])")
            exit(0)
        }
    }
}
