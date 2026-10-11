// Draws Atlas's app icon and builds AppIcon.icns beside this script:
//
//     swift menubar/Resources/AppIcon.swift
//
// The mark is the menu bar glyph's "A" whose free-floating crossbar, the
// load Atlas carries, is system blue, on the macOS rounded-square grid (an
// 824-point tile with a soft shadow in a 1024-point canvas). The "A" is
// drawn again here rather than shared: change it together with
// StatusImage.swift.
import AppKit

func hex(_ value: UInt32) -> NSColor {
    NSColor(
        srgbRed: CGFloat((value >> 16) & 0xff) / 255, green: CGFloat((value >> 8) & 0xff) / 255,
        blue: CGFloat(value & 0xff) / 255, alpha: 1)
}

func drawIcon() {
    let tileRect = NSRect(x: 100, y: 100, width: 824, height: 824)
    let tile = NSBezierPath(roundedRect: tileRect, xRadius: 185, yRadius: 185)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.28)
    shadow.shadowOffset = NSSize(width: 0, height: -10)
    shadow.shadowBlurRadius = 24
    shadow.set()
    NSColor.black.setFill()
    tile.fill()
    NSGraphicsContext.restoreGraphicsState()

    NSGraphicsContext.saveGraphicsState()
    tile.addClip()
    NSGradient(starting: hex(0xFFFFFF), ending: hex(0xE4E4E9))!.draw(in: tile, angle: -90)
    // Upright coordinates: the apex is at the top.
    let left: CGFloat = 240
    let right: CGFloat = 784
    let top: CGFloat = 788
    let bottom: CGFloat = 236
    let apex: CGFloat = 100
    let leg: CGFloat = 96
    let center = (left + right) / 2
    let slope = (center - apex / 2 - left) / (top - bottom)
    let reach = (center - left - leg) / (center - apex / 2 - left)
    let notch = bottom + reach * (top - bottom)
    let letter = NSBezierPath()
    letter.move(to: NSPoint(x: left, y: bottom))
    letter.line(to: NSPoint(x: center - apex / 2, y: top))
    letter.line(to: NSPoint(x: center + apex / 2, y: top))
    letter.line(to: NSPoint(x: right, y: bottom))
    letter.line(to: NSPoint(x: right - leg, y: bottom))
    letter.line(to: NSPoint(x: center, y: notch))
    letter.line(to: NSPoint(x: left + leg, y: bottom))
    letter.close()
    // The crossbar, clear of the legs at its upper (narrower) edge.
    let barBottom: CGFloat = 322
    let barTop: CGFloat = 400
    let gap: CGFloat = 20
    let inside = left + leg + slope * (barTop - bottom) + gap
    let cap = NSBezierPath(
        rect: NSRect(x: inside, y: barBottom, width: 2 * (center - inside), height: barTop - barBottom))
    NSGraphicsContext.saveGraphicsState()
    let lift = NSShadow()
    lift.shadowColor = NSColor.black.withAlphaComponent(0.16)
    lift.shadowOffset = NSSize(width: 0, height: -8)
    lift.shadowBlurRadius = 18
    lift.set()
    hex(0x1C1C1E).setFill()
    letter.fill()
    hex(0x0A84FF).setFill()
    cap.fill()
    NSGraphicsContext.restoreGraphicsState()
    NSGradient(starting: hex(0x48484C), ending: hex(0x1C1C1E))!.draw(in: letter, angle: -90)
    NSGradient(starting: hex(0x4AA3FF), ending: hex(0x0A6FE8))!.draw(in: cap, angle: -90)
    NSGraphicsContext.restoreGraphicsState()

    let rim = NSBezierPath(
        roundedRect: tileRect.insetBy(dx: 1, dy: 1), xRadius: 184, yRadius: 184)
    rim.lineWidth = 2
    NSColor.black.withAlphaComponent(0.06).setStroke()
    rim.stroke()
}

func png(pixels: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: 1024, height: 1024)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    drawIcon()
    NSGraphicsContext.current?.flushGraphics()
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let here = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let iconset = FileManager.default.temporaryDirectory
    .appendingPathComponent("AppIcon-\(ProcessInfo.processInfo.processIdentifier).iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    try png(pixels: points).write(to: iconset.appendingPathComponent("icon_\(points)x\(points).png"))
    try png(pixels: points * 2).write(
        to: iconset.appendingPathComponent("icon_\(points)x\(points)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = [
    "--convert", "icns", "--output", here.appendingPathComponent("AppIcon.icns").path, iconset.path,
]
try iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(at: iconset)
exit(iconutil.terminationStatus)
