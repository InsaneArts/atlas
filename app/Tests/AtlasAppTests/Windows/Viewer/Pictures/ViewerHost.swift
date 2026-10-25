import AppKit
import AtlasUI
import CoreGraphics
import SwiftUI
import Testing

/// The main window's content in an offscreen window (``ViewerHostContent``): a frame read straight after a change
/// shows where things stand as its animation begins, and the views AppKit holds, such as a list's scroll view, can be
/// found and compared across changes.
@MainActor
final class ViewerHost {
  /// The window.
  private let window: NSWindow
  /// The hosted content.
  private let hosting: NSHostingView<ViewerHostContent>

  /// Hosts `state`, laid out once, with nothing to move from.
  ///
  /// - Parameter state: What the window shows first.
  init(_ state: ViewerState) {
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    hosting = NSHostingView(rootView: ViewerHostContent(state: state))
    hosting.frame = NSRect(origin: .zero, size: PreviewStates.viewerSize)
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
  }

  /// Shows `state` and lays the window out once: a change's first frame.
  ///
  /// - Parameter state: What the window shows now.
  func show(_ state: ViewerState) {
    hosting.rootView = ViewerHostContent(state: state)
    hosting.layoutSubtreeIfNeeded()
  }

  /// Which picture is drawn at `point`: the stage's, the corner's, or neither.
  ///
  /// - Parameter point: The point, in the window's points from its top left.
  /// - Returns: The picture's colour, or nil.
  /// - Throws: When nothing was drawn.
  func colour(at point: CGPoint) throws -> PictureColour? {
    try colours(at: [point])[0]
  }

  /// Which picture is drawn at each of `points`, all read from one frame.
  ///
  /// - Parameter points: The points, in the window's points from its top left.
  /// - Returns: Each point's picture colour, or nil.
  /// - Throws: When nothing was drawn.
  func colours(at points: [CGPoint]) throws -> [PictureColour?] {
    let bitmap = try #require(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
    hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
    let sampled = try #require(bitmap.converting(to: .sRGB, renderingIntent: .default))
    let scale = CGFloat(bitmap.pixelsWide) / hosting.bounds.width
    return try points.map { point in
      let colour = try #require(
        sampled.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?
          .usingColorSpace(.sRGB)
      )
      return PictureColour.allCases
        .first { $0.matches(colour) }
    }
  }

  /// 60 ms: the latest a frame may be drawn after a change and still be read as its first; a busy Mac can let an
  /// animation run on for longer before the frame is drawn.
  static let firstFrameWithin: Duration = .milliseconds(60)
  /// 8: how many fresh windows a first-frame read tries before it takes the last frame it drew.
  static let firstFrameTries = 8

  /// Which picture is drawn at each of `points` in the first frame after the window goes from `first` to `next`:
  /// each try hosts `first` in a fresh window, shows `next` and reads one frame, and a frame drawn later than
  /// ``firstFrameWithin`` is not the first, so it tries again, as often as ``firstFrameTries``.
  ///
  /// - Parameters:
  ///   - first: What the window shows before the change.
  ///   - next: What it shows after.
  ///   - points: The points to read, in the window's points from its top left.
  /// - Returns: Each point's picture colour, or nil.
  /// - Throws: When nothing was drawn.
  static func firstFrame(from first: ViewerState, to next: ViewerState, at points: [CGPoint]) throws
    -> [PictureColour?]
  {
    let clock = ContinuousClock()
    var colours: [PictureColour?] = []
    for _ in 0..<firstFrameTries {
      let host = ViewerHost(first)
      let started = clock.now
      host.show(next)
      colours = try host.colours(at: points)
      if clock.now - started <= firstFrameWithin { return colours }
    }
    return colours
  }

  /// What this frame draws in `rect`, as raw pixels, for comparing one frame with another.
  ///
  /// - Parameter rect: The part of the window, in its points from its top left.
  /// - Returns: The pixels.
  /// - Throws: When nothing was drawn.
  func pixels(in rect: CGRect) throws -> Data {
    let bitmap = try #require(hosting.bitmapImageRepForCachingDisplay(in: rect))
    hosting.cacheDisplay(in: rect, to: bitmap)
    return try #require(bitmap.tiffRepresentation)
  }

  /// The scroll views AppKit holds for the content's lists, front to back as the view tree lists them.
  var scrollViews: [NSScrollView] {
    Self.scrollViews(in: hosting)
  }

  /// The scroll views under `view`.
  ///
  /// - Parameter view: A view.
  /// - Returns: Its scroll views and its descendants', in tree order.
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    let own: [NSScrollView] = if let list = view as? NSScrollView { [list] } else { [] }
    return own + view.subviews.flatMap { scrollViews(in: $0) }
  }
}
