import AppKit
import SwiftUI

/// Drawing a view to PNG data at Retina scale, for the design renders.
enum PNG {
  /// The design renders draw two pixels a point, as a Retina display shows them, whatever this Mac's own display.
  static let scale = 2

  /// `view` drawn at Retina scale as a PNG, in `appearance`.
  ///
  /// - Parameters:
  ///   - view: The view, at its own size.
  ///   - appearance: The appearance to draw it in.
  /// - Returns: The PNG's bytes, or nil when no bitmap could be made.
  static func data(of view: NSView, appearance: NSAppearance) -> Data? {
    view.appearance = appearance
    view.layoutSubtreeIfNeeded()
    let size = view.bounds.size
    guard
      let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: Int(size.width) * scale,
        pixelsHigh: Int(size.height) * scale,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
      )
    else { return nil }
    bitmap.size = size
    appearance.performAsCurrentDrawingAppearance {
      view.cacheDisplay(in: view.bounds, to: bitmap)
    }
    return bitmap.representation(using: .png, properties: [:])
  }

  /// The hosting view every render draws through: the shell's bare hosting view with the render switches set. A view
  /// is drawn once, before any animation runs, so arrivals are turned off and each view that plays one is drawn
  /// settled; and AppKit draws nothing under the system glass, so glass is drawn as its material.
  ///
  /// - Parameter view: The SwiftUI view.
  /// - Returns: Its hosting view, ready to size and draw.
  static func host(_ view: some View) -> NSView {
    WindowShell.hostingView(
      view
        .environment(\.animatesArrivals, false)
        .environment(\.drawsGlassEffect, false)
    )
  }

  /// Draws `view` at `size`, in Atlas's dark appearance, to the PNG at `file`, through ``host(_:)``.
  ///
  /// - Parameters:
  ///   - view: The SwiftUI view.
  ///   - size: Its window's size.
  ///   - file: Where the PNG goes.
  /// - Returns: Whether the PNG was written.
  static func write(_ view: some View, size: NSSize, to file: URL) -> Bool {
    let host = host(view)
    host.frame = NSRect(origin: .zero, size: size)
    guard let appearance = Appearance.dark, let data = data(of: host, appearance: appearance) else {
      return false
    }
    return (try? data.write(to: file)) != nil
  }
}
