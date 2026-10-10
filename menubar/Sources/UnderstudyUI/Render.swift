import AppKit
import UnderstudyKit

/// Draw the panel and its menu bar glyph, as a person would see them, to a
/// PNG. Used for visual checks without capturing the host screen.
public func render(viewer: Viewer, to path: String, appearance name: NSAppearance.Name) {
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
    group.notify(queue: .main) {
        let appearance = NSAppearance(named: name)!
        let snapshot = status.map(Snapshot.init(status:)) ?? Snapshot()
        let content = panel.view
        content.appearance = appearance
        panel.show(snapshot)
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
        let glyph = NSImageView(image: statusImage(Mode.of(snapshot.mode)))
        glyph.frame = NSRect(x: size.width - 40, y: size.height + 14, width: 22, height: 16)
        canvas.addSubview(glyph)
        content.frame = NSRect(origin: .zero, size: size)
        canvas.addSubview(content)
        canvas.layoutSubtreeIfNeeded()
        guard let bitmap = canvas.bitmapImageRepForCachingDisplay(in: canvas.bounds) else {
            exit(3)
        }
        appearance.performAsCurrentDrawingAppearance {
            canvas.cacheDisplay(in: canvas.bounds, to: bitmap)
        }
        guard let png = bitmap.representation(using: .png, properties: [:]),
            (try? png.write(to: URL(fileURLWithPath: path))) != nil
        else { exit(4) }
        exit(0)
    }
}
