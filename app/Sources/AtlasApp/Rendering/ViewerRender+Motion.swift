#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  extension ViewerRender {
    /// The key frames of the main window's transitions (``PreviewStates/viewerMotion``), each pose at each step, by
    /// file name, in order.
    static var motionNames: [String] {
      PreviewStates.viewerMotion
        .flatMap { pose in pose.steps.map(pose.file(at:)) }
    }

    /// The transitions' names, in order, for a test to list as its arguments.
    static var motionPoseNames: [String] {
      PreviewStates.viewerMotion
        .map(\.name)
    }

    /// Draws every transition's key frames to PNGs in `folder`, at Retina scale.
    ///
    /// - Parameters:
    ///   - folder: Where the PNGs go.
    ///   - appearance: The appearance to draw them in, light or dark.
    /// - Returns: Whether every frame was drawn.
    static func drawMotion(to folder: URL, appearance: NSAppearance?) -> Bool {
      PreviewStates.viewerMotion
        .flatMap { pose in pose.steps.map { (pose, $0) } }
        .map { draw($0.0, at: $0.1, to: folder, appearance: appearance) }
        .allSatisfy { $0 }
    }

    /// Draws one key frame of `pose` to its PNG in `folder`: the window shows the pose's first state held well before
    /// the change and settles there, so every animation it makes is posed, a row's way out included; then it takes its
    /// second, in the animation the change runs in, while every animation is held at the change, and the held moment
    /// moves on a tick at a time to the step's, synchronously rendering each tick so every animation samples it
    /// before it advances. The window is offscreen and its own, so nothing of one frame carries into the next.
    ///
    /// - Parameters:
    ///   - pose: The transition.
    ///   - step: The share of its leading move, from 0 to 1.
    ///   - folder: Where the PNG goes.
    ///   - appearance: The appearance to draw it in, light or dark.
    /// - Returns: Whether it was drawn.
    static func draw(_ pose: ViewerMotionPose, at step: Double, to folder: URL, appearance: NSAppearance?) -> Bool {
      guard let appearance else { return false }
      let window = NSWindow(
        contentRect: NSRect(origin: .zero, size: pose.size),
        styleMask: [.borderless],
        backing: .buffered,
        defer: false
      )
      window.isReleasedWhenClosed = false
      window.appearance = appearance
      let hosting = WindowShell.hostingView(AnyView(ViewerMotionFrame(pose, isChanged: false)))
      hosting.frame = NSRect(origin: .zero, size: pose.size)
      Motion.Pose.hold(at: -Self.before)
      defer { Motion.Pose.hold(at: nil) }
      window.contentView = hosting
      turn(hosting)
      let moment = pose.moment(at: step)
      var held = TimeInterval.zero
      Motion.Pose.hold(at: held)
      // Held at the change, what the first state started long before it has ended.
      for _ in 0..<settleTurns { turn(hosting) }
      hosting.rootView = AnyView(ViewerMotionFrame(pose, isChanged: true))
      turn(hosting)
      while held < moment {
        held = min(held + ViewerMotionPose.tick, moment)
        Motion.Pose.hold(at: held)
        turn(hosting)
      }
      // Held still at the frame's moment, the content draws what the last tick finished.
      for _ in 0..<settleTurns { turn(hosting) }
      let data = PNG.data(of: hosting, appearance: appearance)
      // Gone with its content, the window leaves nothing running into the next frame.
      window.contentView = nil
      window.close()
      guard let data else { return false }
      // The render reports a failed write as false; why it failed is not its concern.
      return (try? data.write(to: folder.appendingPathComponent("\(pose.file(at: step)).png"))) != nil
    }

    /// Renders the held moment synchronously, including the changes its handlers and animation completions make.
    /// Waiting for a display frame on the real run loop can skip a held tick or first sample an animation on a later
    /// one, shifting its start. SwiftUI's test renderer advances explicitly; the zero interval drains the resulting
    /// changes without advancing that clock again.
    ///
    /// - Parameter hosting: The window's content.
    private static func turn(_ hosting: NSHostingView<AnyView>) {
      hosting.needsLayout = true
      hosting.layoutSubtreeIfNeeded()
      hosting._renderForTest(interval: ViewerMotionPose.tick)
      hosting._renderForTest(interval: .zero)
      hosting.layoutSubtreeIfNeeded()
    }

    /// 5 s: how long before the change the first state is drawn, past the end of any animation it starts.
    private static let before: TimeInterval = 5

    /// 3: how many more times the content renders once the frame's moment is held, so what the last tick ended, and
    /// what that started, is drawn.
    private static let settleTurns = 3
  }
#endif
