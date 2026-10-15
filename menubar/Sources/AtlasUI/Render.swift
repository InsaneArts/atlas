import AppKit
import AtlasKit

/// Draw the panel and its menu bar glyph, as a person would see them, to a
/// PNG at Retina scale. Used for visual checks without capturing the host
/// screen. For design checks, `ATLAS_RENDER_PREVIEW=1` shows every
/// action as on a set-up Mac, `ATLAS_RENDER_MODE` overrides the mode
/// and `ATLAS_RENDER_MESSAGE` shows a feedback line.
public func render(viewer: Viewer, to path: String, appearance name: NSAppearance.Name) {
    let environment = ProcessInfo.processInfo.environment
    let panel = PanelController(viewer: viewer, commands: nil, loginItem: nil)
    nonisolated(unsafe) var status: [String: Any]?
    nonisolated(unsafe) var frame: Data?
    let group = DispatchGroup()
    group.enter()
    viewer.status {
        status = $0
        group.leave()
    }
    group.enter()
    viewer.frame {
        frame = $0
        group.leave()
    }
    group.notify(queue: .main) { MainActor.assumeIsolated {
        let appearance = NSAppearance(named: name)!
        var snapshot = status.map(Snapshot.init(status:)) ?? Snapshot()
        if let mode = environment["ATLAS_RENDER_MODE"], !mode.isEmpty {
            snapshot.mode = mode
        }
        let content = panel.view
        content.appearance = appearance
        if environment["ATLAS_RENDER_PREVIEW"] == "1" { panel.preview() }
        panel.show(snapshot)
        if let message = environment["ATLAS_RENDER_MESSAGE"], !message.isEmpty {
            panel.say(message)
        }
        panel.thumbnail.image = frame.flatMap(NSImage.init(data:))
        content.layoutSubtreeIfNeeded()
        let size = content.fittingSize
        let bar: CGFloat = 28
        let canvas = NSView(
            frame: NSRect(x: 0, y: 0, width: size.width, height: size.height + bar + 8))
        canvas.appearance = appearance
        let background = NSBox(frame: canvas.bounds)
        background.boxType = .custom
        background.borderWidth = 0
        background.fillColor = .windowBackgroundColor
        canvas.addSubview(background)
        let strip = NSBox(frame: NSRect(x: 0, y: size.height + 8, width: size.width, height: bar))
        strip.boxType = .custom
        strip.borderWidth = 0
        strip.fillColor = .underPageBackgroundColor
        canvas.addSubview(strip)
        // The glyph at its own size, centred in the strip as the menu bar does.
        let image = statusImage(Mode.of(snapshot.mode))
        let glyph = NSImageView(image: image)
        glyph.imageScaling = .scaleNone
        glyph.frame = NSRect(
            x: (size.width - 40).rounded(),
            y: (size.height + 8 + (bar - image.size.height) / 2).rounded(.down),
            width: image.size.width, height: image.size.height)
        canvas.addSubview(glyph)
        content.frame = NSRect(origin: .zero, size: size)
        canvas.addSubview(content)
        canvas.layoutSubtreeIfNeeded()
        let scale = 2
        guard
            let bitmap = NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: Int(canvas.bounds.width) * scale,
                pixelsHigh: Int(canvas.bounds.height) * scale, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0)
        else { exit(3) }
        bitmap.size = canvas.bounds.size
        appearance.performAsCurrentDrawingAppearance {
            canvas.cacheDisplay(in: canvas.bounds, to: bitmap)
        }
        guard let png = bitmap.representation(using: .png, properties: [:]),
            (try? png.write(to: URL(fileURLWithPath: path))) != nil
        else { exit(4) }
        exit(0)
    } }
}
