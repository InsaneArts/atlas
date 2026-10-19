import AppKit
import AtlasKit
import SwiftUI

/// Draw the panel and its menu bar glyph, as a person would see them, to a
/// PNG at `path`, from what the viewer service reports now. For design
/// checks, `ATLAS_RENDER_PREVIEW=1` shows every action as on a set-up Mac,
/// `ATLAS_RENDER_MODE` overrides the mode, `ATLAS_RENDER_MESSAGE` shows
/// a feedback line and `ATLAS_RENDER_LOGOUT` (`ask` or `held`) shows the
/// log-out confirmation or the offer to force one.
@MainActor
public func renderPanel(service: any ViewerService, to path: String, appearance name: NSAppearance.Name) async -> Bool {
    let environment = ProcessInfo.processInfo.environment
    async let reading = try? service.status()
    async let picture = try? service.frame()
    var status = await reading ?? ViewerStatus()
    if let mode = environment["ATLAS_RENDER_MODE"].flatMap(HelperMode.init(rawValue:)) {
        status.overview.mode = mode
    }
    let message = environment["ATLAS_RENDER_MESSAGE"].flatMap { $0.isEmpty ? nil : $0 }
    let shown = await picture.flatMap { NSImage(data: $0.png) }
    let logOut = environment["ATLAS_RENDER_LOGOUT"]
    let model = PanelModel.still(
        status, image: shown, message: message,
        everything: logOut != nil || environment["ATLAS_RENDER_PREVIEW"] == "1",
        confirming: logOut == "ask", forceOffered: logOut == "held")
    let content = NSHostingView(rootView: PanelView(model: model))
    let size = content.fittingSize
    let bar: CGFloat = 28
    let canvas = NSView(frame: NSRect(x: 0, y: 0, width: size.width, height: size.height + bar + 8))
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
    let image = statusImage(model.mode)
    let glyph = NSImageView(image: image)
    glyph.imageScaling = .scaleNone
    glyph.frame = NSRect(
        x: (size.width - 40).rounded(),
        y: (size.height + 8 + (bar - image.size.height) / 2).rounded(.down),
        width: image.size.width, height: image.size.height)
    canvas.addSubview(glyph)
    content.frame = NSRect(origin: .zero, size: size)
    canvas.addSubview(content)
    guard let appearance = NSAppearance(named: name), let data = png(of: canvas, appearance: appearance) else {
        return false
    }
    return (try? data.write(to: URL(fileURLWithPath: path))) != nil
}
