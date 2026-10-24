// Draws Atlas's app icon and builds, beside this script, AppIcon.icns, the
// mark layer of the Icon Composer icon AppIcon.icon/Assets/Mark.png, and
// the helper account's picture, HelperPicture.png:
//
//     swift app/Resources/AppIcon.swift
//
// The mark is Atlas's orbit, a flat black planet with a ring tilted across
// it: one ink, no shading. The ring's front half passes over the sphere,
// which is cut clear on either side of it; the ring itself is whole. The
// mark spans 73.5% of the tile's width. macOS 26 and later shape and light
// the icon themselves, from the mark alone on the plain white fill
// AppIcon.icon/icon.json names; an icon that brings its own tile is shrunk
// inside a second one there. Earlier macOS shows AppIcon.icns: the mark on
// a white tile on the rounded-square grid (an 824-point tile with a soft
// shadow in a 1024-point canvas). The account picture is the same mark on
// white filling the canvas, which macOS shows in a circle. The menu bar
// glyph (Shared/MenuBar/StatusGlyph.swift), the header mark
// (OrbitMark.swift), the plugin logo (agents/atlas/assets/atlas.svg) and the
// README logo (scripts/logo.mjs) draw the same shape: change them together.
import AppKit

let canvas = NSRect(x: 0, y: 0, width: 1024, height: 1024)

func drawIcon(pixel: CGFloat) {
    let tileRect = NSRect(x: 100, y: 100, width: 824, height: 824)
    let tile = NSBezierPath(roundedRect: tileRect, xRadius: 185, yRadius: 185)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.28)
    shadow.shadowOffset = NSSize(width: 0, height: -10)
    shadow.shadowBlurRadius = 24
    shadow.set()
    NSColor.white.setFill()
    tile.fill()
    NSGraphicsContext.restoreGraphicsState()

    drawOrbit(width: 0.735 * tileRect.width, pixel: pixel)

    let rim = NSBezierPath(
        roundedRect: tileRect.insetBy(dx: 1, dy: 1), xRadius: 184, yRadius: 184)
    rim.lineWidth = 2
    NSColor.black.withAlphaComponent(0.06).setStroke()
    rim.stroke()
}

/// The account picture: the mark on white filling the whole canvas.
func drawPicture(pixel: CGFloat) {
    NSColor.white.setFill()
    canvas.fill()
    drawOrbit(width: 0.735 * canvas.width, pixel: pixel)
}

/// The Icon Composer layer: the mark alone, clear around it and in its gap
/// so the icon's own fill shows through. The canvas is the whole tile there.
func drawMark(pixel: CGFloat) {
    drawOrbit(width: 0.735 * canvas.width, pixel: pixel)
}

/// The orbit in black, `width` points wide (the ring's outer reach) and
/// centred in the canvas, from its 64-unit drawing (y grows downward): a
/// sphere of radius 14.6 and a ring of 25.6 by 6.9 tilted 20 degrees, right
/// side up, drawn 2.6 units wide but never under 1.2 pixels of `pixel`
/// points. The gap is 2.2 lines wide, along the ring's front half.
func drawOrbit(width: CGFloat, pixel: CGFloat) {
    let scale = width / 50.8
    let line = max(2.6 * scale, 1.2 * pixel)
    // Upright coordinates: flip the drawing's downward y.
    var place = CGAffineTransform(translationX: canvas.midX, y: canvas.midY).scaledBy(x: scale, y: -scale)
    let sphere = CGPath(ellipseIn: CGRect(x: -14.6, y: -14.6, width: 29.2, height: 29.2), transform: &place)
    var tilt = place.rotated(by: -20 * .pi / 180).scaledBy(x: 25.6, y: 6.9)
    let ring = CGPath(ellipseIn: CGRect(x: -1, y: -1, width: 2, height: 2), transform: &tilt)
    // The lower half on screen, which passes in front of the sphere.
    let front = CGMutablePath()
    front.addArc(center: .zero, radius: 1, startAngle: 0, endAngle: .pi, clockwise: false, transform: tilt)

    let context = NSGraphicsContext.current!.cgContext
    context.saveGState()
    context.setFillColor(.black)
    context.setStrokeColor(.black)
    context.setLineWidth(line)
    context.addPath(ring)
    context.strokePath()
    // The sphere in a layer of its own, so the gap clears only it and the
    // back of the ring stays hidden behind it.
    context.saveGState()
    context.beginTransparencyLayer(auxiliaryInfo: nil)
    context.addPath(sphere)
    context.fillPath()
    context.setBlendMode(.clear)
    context.setLineWidth(2.2 * line)
    context.addPath(front)
    context.strokePath()
    context.endTransparencyLayer()
    context.restoreGState()
    context.addPath(front)
    context.strokePath()
    context.restoreGState()
}

func png(pixels: Int, draw: (CGFloat) -> Void = drawIcon) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = canvas.size
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(canvas.width / CGFloat(pixels))
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
// The size macOS keeps for account pictures.
try png(pixels: 512, draw: drawPicture).write(to: here.appendingPathComponent("HelperPicture.png"))
try png(pixels: 1024, draw: drawMark).write(
    to: here.appendingPathComponent("AppIcon.icon/Assets/Mark.png"))
exit(iconutil.terminationStatus)
