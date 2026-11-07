// `openstitch-quicklook-check HELPER INPUT OUTPUT.png [SIZE]`: runs Renderer.swift, the
// extension's code path to the Rust renderer, outside Quick Look, for the CI check (#557).
import Foundation

@main
struct Check {
    static func main() {
        let args = CommandLine.arguments
        guard args.count >= 4 else {
            FileHandle.standardError.write(
                Data("usage: openstitch-quicklook-check HELPER INPUT OUTPUT.png [SIZE]\n".utf8))
            exit(64)
        }
        do {
            let input = URL(fileURLWithPath: args[2])
            let png = try Renderer.png(
                data: try Data(contentsOf: input), fileExtension: input.pathExtension,
                size: args.count > 4 ? Int(args[4]) ?? 256 : 256,
                helper: URL(fileURLWithPath: args[1]))
            try png.write(to: URL(fileURLWithPath: args[3]))
            print("wrote \(args[3]) (\(png.count) bytes)")
        } catch {
            FileHandle.standardError.write(Data("openstitch-quicklook-check: \(error)\n".utf8))
            exit(1)
        }
    }
}
