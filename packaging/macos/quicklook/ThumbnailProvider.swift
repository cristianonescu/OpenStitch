// The Quick Look thumbnail extension's entry point (#557): Finder asks for a thumbnail, the
// Rust renderer draws it (Renderer.swift), and the PNG is drawn into Quick Look's context.
import CoreGraphics
import Foundation
import ImageIO
import QuickLookThumbnailing

@objc(ThumbnailProvider)
final class ThumbnailProvider: QLThumbnailProvider {
    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        let side = max(request.maximumSize.width, request.maximumSize.height)
        let pixels = Int((side * request.scale).rounded(.up))
        do {
            guard let helper = Bundle.main.url(forAuxiliaryExecutable: "openstitch-thumbnailer")
            else { throw ThumbnailError.helperMissing }
            let data = try Data(contentsOf: request.fileURL, options: .mappedIfSafe)
            let png = try Renderer.png(
                data: data, fileExtension: request.fileURL.pathExtension, size: pixels,
                helper: helper)
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
            handler(nil, error)
        }
    }
}
