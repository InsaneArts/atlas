import AppKit
import SwiftUI

/// `view` drawn at Retina scale as a PNG, in `appearance`.
@MainActor
func png(of view: NSView, appearance: NSAppearance) -> Data? {
    view.appearance = appearance
    view.layoutSubtreeIfNeeded()
    let size = view.bounds.size
    guard
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(size.width) * 2, pixelsHigh: Int(size.height) * 2,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    else { return nil }
    bitmap.size = size
    appearance.performAsCurrentDrawingAppearance {
        view.cacheDisplay(in: view.bounds, to: bitmap)
    }
    return bitmap.representation(using: .png, properties: [:])
}

/// Draw `view` at `size`, in Atlas's dark appearance, to the PNG at `file`.
@MainActor
func draw(_ view: some View, size: NSSize, to file: URL) -> Bool {
    let host = NSHostingView(rootView: view)
    host.frame = NSRect(origin: .zero, size: size)
    guard let appearance = NSAppearance(named: .darkAqua), let data = png(of: host, appearance: appearance) else {
        return false
    }
    return (try? data.write(to: file)) != nil
}
