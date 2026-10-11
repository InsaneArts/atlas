// Draws Atlas's app icon and builds AppIcon.icns beside this script:
//
//     swift menubar/Resources/AppIcon.swift
//
// The mark is Atlas's orbit, a graphite sphere with a ring passing in front
// of it, on a white tile on the macOS rounded-square grid (an 824-point tile
// with a soft shadow in a 1024-point canvas). The menu bar glyph in
// StatusImage.swift draws the same shape: change them together.
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

    let paper = NSGradient(starting: hex(0xFFFFFF), ending: hex(0xE1E4E9))!
    NSGraphicsContext.saveGraphicsState()
    tile.addClip()
    paper.draw(in: tile, angle: -90)

    // Upright coordinates. The ring rises to the right; its front half is
    // the lower one.
    let center = NSPoint(x: 512, y: 504)
    func ring(from start: CGFloat, to end: CGFloat) -> NSBezierPath {
        let path = NSBezierPath()
        path.appendArc(
            withCenter: .zero, radius: 1, startAngle: start, endAngle: end, clockwise: false)
        var tilt = AffineTransform(translationByX: center.x, byY: center.y)
        tilt.rotate(byDegrees: 20)
        tilt.scale(x: 372, y: 112)
        path.transform(using: tilt)
        return path
    }
    func stroke(_ path: NSBezierPath, width: CGFloat) -> CGPath {
        path.cgPath.copy(strokingWithWidth: width, lineCap: .butt, lineJoin: .miter, miterLimit: 10)
    }
    let ink = NSGradient(colors: [hex(0x1C1C1E), hex(0x48484C), hex(0x1C1C1E)])!
    let context = NSGraphicsContext.current!.cgContext

    // The back of the ring, then the sphere over it.
    hex(0x1C1C1E).withAlphaComponent(0.9).setFill()
    NSBezierPath(cgPath: stroke(ring(from: 0, to: 360), width: 30)).fill()
    let sphere = NSBezierPath(
        ovalIn: NSRect(x: center.x - 212, y: center.y - 212, width: 424, height: 424))
    let light = NSPoint(x: 444, y: 597)
    NSGraphicsContext.saveGraphicsState()
    sphere.addClip()
    NSGradient(
        colors: [hex(0x8E8E94), hex(0x3A3A3E), hex(0x0F0F11)], atLocations: [0, 0.42, 1],
        colorSpace: .sRGB)!
        .draw(
            fromCenter: light, radius: 0, toCenter: light, radius: 382,
            options: .drawsAfterEndingLocation)
    NSGraphicsContext.restoreGraphicsState()

    // The paper shows through a gap where the ring's front crosses the sphere.
    NSGraphicsContext.saveGraphicsState()
    NSBezierPath(
        ovalIn: NSRect(x: center.x - 232, y: center.y - 232, width: 464, height: 464)
    ).addClip()
    context.addPath(stroke(ring(from: 180, to: 360), width: 64))
    context.clip()
    paper.draw(in: tile, angle: -90)
    NSGraphicsContext.restoreGraphicsState()

    // The front of the ring.
    NSGraphicsContext.saveGraphicsState()
    context.addPath(stroke(ring(from: 180, to: 360), width: 30))
    context.clip()
    ink.draw(in: NSRect(x: 130, y: 300, width: 764, height: 420), angle: 0)
    NSGraphicsContext.restoreGraphicsState()
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
