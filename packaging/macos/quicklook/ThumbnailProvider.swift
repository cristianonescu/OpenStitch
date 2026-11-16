// The Quick Look thumbnail extension's entry point (#557): Finder asks for a thumbnail, the
// Rust renderer linked into the extension draws it (Renderer.swift, #1182), and the PNG is
// drawn into Quick Look's context. A failure is logged (subsystem
// io.github.cristianonescu.openstitch.thumbnail) and Finder keeps the type's icon.
import CoreGraphics
import Foundation
import ImageIO
import QuickLookThumbnailing
import os

private let log = Logger(
    subsystem: "io.github.cristianonescu.openstitch.thumbnail", category: "thumbnail")

@objc(ThumbnailProvider)
final class ThumbnailProvider: QLThumbnailProvider {
    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        let side = max(request.maximumSize.width, request.maximumSize.height)
        let pixels = Int((side * request.scale).rounded(.up))
        do {
            let data = try Data(contentsOf: request.fileURL, options: .mappedIfSafe)
            let png = try Renderer.png(
                data: data, fileExtension: request.fileURL.pathExtension, size: pixels)
            guard let source = CGImageSourceCreateWithData(png as CFData, nil),
                let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
            else { throw ThumbnailError.notAPicture }
            let box = CGSize(width: side, height: side)
            handler(
                QLThumbnailReply(contextSize: box) { context in
                    context.draw(image, in: CGRect(origin: .zero, size: box))
                    return true
                }, nil)
        } catch {
            log.error("no thumbnail: \(String(describing: error), privacy: .public)")
            handler(nil, error)
        }
    }
}
