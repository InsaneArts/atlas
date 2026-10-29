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
  /// - Parameters:
  ///   - state: What the window shows first.
  ///   - animatesArrivals: Whether the first pane plays its arrival; false for a settled reference.
  init(_ state: ViewerState, animatesArrivals: Bool = true) {
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    hosting = NSHostingView(rootView: ViewerHostContent(state: state, animatesArrivals: animatesArrivals))
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

  /// Lays the window out again as it stands, as after a turn of the run loop.
  func layOut() {
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
    try firstFrame(from: first, to: next) { try $0.colours(at: points) }
  }

  /// What `read` finds in the first frame after the window goes from `first` to `next`, tried in fresh windows as
  /// ``firstFrame(from:to:at:)`` tries them.
  ///
  /// - Parameters:
  ///   - first: What the window shows before the change.
  ///   - next: What it shows after.
  ///   - animatesArrivals: Whether the first pane plays its arrival; false for a settled reference.
  ///   - read: Reads the frame.
  /// - Returns: What `read` found.
  /// - Throws: What `read` throws.
  static func firstFrame<Found>(
    from first: ViewerState,
    to next: ViewerState,
    animatesArrivals: Bool = true,
    read: (ViewerHost) throws -> Found
  ) throws -> Found {
    let clock = ContinuousClock()
    var found: Found?
    for _ in 0..<firstFrameTries {
      let host = ViewerHost(first, animatesArrivals: animatesArrivals)
      let started = clock.now
      host.show(next)
      found = try read(host)
      if clock.now - started <= firstFrameWithin, let found { return found }
    }
    return try #require(found)
  }

  /// How bright this frame draws each of `points`, from 0 for black to 1 for white, all read from one frame.
  ///
  /// - Parameter points: The points, in the window's points from its top left.
  /// - Returns: Each point's brightness.
  /// - Throws: When nothing was drawn.
  func brightness(at points: [CGPoint]) throws -> [CGFloat] {
    let bitmap = try #require(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
    hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
    let sampled = try #require(bitmap.converting(to: .sRGB, renderingIntent: .default))
    let scale = CGFloat(bitmap.pixelsWide) / hosting.bounds.width
    return try points.map { point in
      let colour = try #require(
        sampled.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?
          .usingColorSpace(.sRGB)
      )
      return colour.brightnessComponent
    }
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

  /// The scroll views AppKit holds for the stage's lists, front to back as the view tree lists them: those in the card,
  /// past the sidebar, whose sessions scroll too.
  var scrollViews: [NSScrollView] {
    Self.scrollViews(in: hosting)
      .filter { $0.convert($0.bounds, to: hosting).minX >= Self.cardEdge }
  }

  /// The scroll views AppKit holds in the sidebar, whose sessions scroll in their own room.
  var sidebarScrollViews: [NSScrollView] {
    Self.scrollViews(in: hosting)
      .filter { $0.convert($0.bounds, to: hosting).minX < Self.cardEdge }
  }

  /// Where `view` stands, in the window's points from its top left.
  ///
  /// - Parameter view: A view in the window.
  /// - Returns: Its frame.
  func frameFromTop(of view: NSView) -> CGRect {
    let frame = view.convert(view.bounds, to: hosting)
    guard !hosting.isFlipped else { return frame }
    return CGRect(x: frame.minX, y: hosting.bounds.height - frame.maxY, width: frame.width, height: frame.height)
  }

  /// 236: where the card starts, past the sidebar, whose own inset keeps the two apart.
  static let cardEdge: CGFloat = 236

  /// The scroll views under `view`.
  ///
  /// - Parameter view: A view.
  /// - Returns: Its scroll views and its descendants', in tree order.
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    let own: [NSScrollView] = if let list = view as? NSScrollView { [list] } else { [] }
    return own + view.subviews.flatMap { scrollViews(in: $0) }
  }
}
