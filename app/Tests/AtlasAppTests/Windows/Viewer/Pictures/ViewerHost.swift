import AppKit
import AsyncSupportTesting
import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

/// The main window's content in an offscreen window (``ViewerHostContent``): a frame read straight after a change
/// shows where things stand as its animation begins, and the views AppKit holds, such as a list's scroll view, can be
/// found and compared across changes.
@MainActor
final class ViewerHost {
  /// The window.
  private let window: NSWindow
  /// The hosted content.
  private let hosting: NSHostingView<ViewerHostContent>
  private let reduceMotion: Bool
  private let placement: CardScrollPlacement
  private var frameReport: LayoutFrameReport?
  /// The latest real layout frames reported by named elements.
  private(set) var frames: [String: CGRect] = [:]

  /// Hosts `state`, laid out once, with nothing to move from.
  ///
  /// - Parameters:
  ///   - state: What the window shows first.
  ///   - animatesArrivals: Whether the first pane plays its arrival; false for a settled reference.
  ///   - reduceMotion: Whether layout must settle without movement.
  ///   - appearance: The window's appearance; nil follows the Mac.
  ///   - placement: How the list defers final selection placement.
  init(
    _ state: ViewerState,
    animatesArrivals: Bool = true,
    reduceMotion: Bool = false,
    appearance: NSAppearance? = nil,
    placement: CardScrollPlacement = CardScrollPlacement()
  ) {
    self.reduceMotion = reduceMotion
    self.placement = placement
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.appearance = appearance
    hosting = NSHostingView(
      rootView: ViewerHostContent(
        state: state,
        animatesArrivals: animatesArrivals,
        reduceMotion: reduceMotion,
        placement: placement
      )
    )
    frameReport = LayoutFrameReport { [weak self] id, frame in self?.frames[id] = frame }
    hosting.rootView = ViewerHostContent(
      state: state,
      animatesArrivals: animatesArrivals,
      reduceMotion: reduceMotion,
      placement: placement,
      frameReport: frameReport
    )
    hosting.frame = NSRect(origin: .zero, size: PreviewStates.viewerSize)
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
  }

  /// Shows `state` and lays the window out once: a change's first frame.
  ///
  /// - Parameters:
  ///   - state: What the window shows now.
  ///   - animatesArrivals: Whether pane arrivals play while testing this change.
  func show(_ state: ViewerState, animatesArrivals: Bool = true) {
    hosting.rootView = ViewerHostContent(
      state: state,
      animatesArrivals: animatesArrivals,
      reduceMotion: reduceMotion,
      placement: placement,
      frameReport: frameReport
    )
    hosting.layoutSubtreeIfNeeded()
  }

  /// Advances the real SwiftUI renderer while animation time remains explicitly held by the test.
  func render() {
    hosting.needsLayout = true
    hosting.layoutSubtreeIfNeeded()
    hosting._renderForTest(interval: ViewerMotionPose.tick)
    hosting._renderForTest(interval: .zero)
    hosting.layoutSubtreeIfNeeded()
  }

  /// Flushes layout and drawing until the requested state is observable, without advancing held animation time.
  /// The ceiling only diagnoses missing work; a passing test never waits for elapsed wall time.
  ///
  /// - Parameters:
  ///   - failure: What failed to become ready.
  ///   - isReady: Whether layout or drawing has reached the state to sample.
  /// - Throws: A failed requirement if the state never arrives, or a sampling failure.
  func render(until failure: String, _ isReady: () throws -> Bool) throws {
    let bound = ContinuousClock.now
      .advanced(by: Ceiling.work)
    while try !isReady() {
      try #require(ContinuousClock.now < bound, Comment(rawValue: failure))
      render()
    }
    render()
  }

  /// The entire frame at the design renderer's Retina scale, with the current animation moment still held.
  ///
  /// - Returns: Its bitmap.
  /// - Throws: When the frame cannot be drawn or decoded.
  func bitmap() throws -> NSBitmapImageRep {
    let data = try #require(PNG.data(of: hosting, appearance: window.effectiveAppearance))
    return try #require(NSBitmapImageRep(data: data))
  }

  /// Presents the test window only for opt-in native focus and scrolling checks run in the hidden helper.
  func presentNativeWindow() {
    window.makeKeyAndOrderFront(nil)
  }

  /// Closes the window a native helper check presented.
  func closeNativeWindow() {
    window.close()
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

  /// Which picture is drawn at each of `points` in the first frame after the window goes from `first` to `next`:
  /// animation time is held at zero, regardless of how long layout or drawing takes.
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

  /// What `read` finds with the real animations held at their first frame.
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
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(first, animatesArrivals: animatesArrivals)
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    host.show(next)
    host.render()
    return try read(host)
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
