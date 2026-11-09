import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Footer slot motion", .serialized, AppKitState())
@MainActor
struct FooterSlotMotionTests {
  @Test(
    "the sound button never slides over Stop while Stop is drawn, as Stop comes or goes",
    arguments: [true, false]
  )
  func theSoundButtonNeverCrossesStop(stopComes: Bool) throws {
    let working = PreviewStates.viewerWorking
    let control = PreviewStates.viewerControl
    let frames = try Self.frames(from: stopComes ? control : working, to: stopComes ? working : control)
    // The sound button crosses Stop's place on its way, and Stop is drawn at one end of the change.
    #expect(frames.contains { $0.soundInStop }, "the sound button never crossed Stop's place: \(frames)")
    #expect(frames.contains { $0.stopDrawn }, "Stop was never drawn: \(frames)")
    let crossing = frames.filter { $0.soundInStop && $0.stopDrawn }
    #expect(crossing.isEmpty, "the sound button was drawn over Stop: \(crossing)")
  }

  /// One frame of a change in the footer: whether Stop's mark is drawn, and whether the sound button's mark is
  /// drawn in Stop's capsule, clear of Stop's own mark and word.
  struct Frame: CustomStringConvertible {
    /// Seconds since the change.
    let moment: TimeInterval
    /// Whether Stop's mark is drawn.
    let stopDrawn: Bool
    /// Whether the sound button's mark is drawn inside Stop's capsule.
    let soundInStop: Bool

    /// The frame, for a failure to say.
    var description: String {
      String(format: "%.3f s: Stop %@, sound %@", moment, stopDrawn ? "drawn" : "-", soundInStop ? "in Stop" : "-")
    }
  }

  /// The centre of Stop's square in a standard window, in points from its top left.
  static let stopMark = CGPoint(x: 1215.5, y: 870)
  /// Points in Stop's capsule, before its square and after its word, where only another button's mark draws: across
  /// the band the sound button's mark takes.
  static let stopRoom: [CGPoint] = [1189, 1193, 1197, 1201, 1205, 1260, 1264, 1268, 1272, 1276, 1280]
    .flatMap { x in [866, 870, 874].map { y in CGPoint(x: x, y: y) } }
  /// 0.5 seconds: past the end of the footer's change.
  static let end: TimeInterval = 0.5
  /// Brightness above which a mark is drawn on the dark card.
  static let drawn: CGFloat = 0.4

  /// Every frame of the dark window's change from `first` to `next`, a tick apart, as a display draws it.
  ///
  /// - Parameters:
  ///   - first: What the window shows before the change.
  ///   - next: What it shows after.
  /// - Returns: The frames.
  /// - Throws: When nothing was drawn.
  static func frames(from first: ViewerState, to next: ViewerState) throws -> [Frame] {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(first, appearance: NSAppearance(named: .darkAqua))
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    host.show(next)
    host.render()
    return try stride(from: TimeInterval.zero, through: end, by: ViewerMotionPose.tick)
      .map { moment in
        Motion.Pose.hold(at: moment)
        host.render()
        let lit = try host.brightness(at: [stopMark] + stopRoom)
          .map { $0 > drawn }
        return Frame(moment: moment, stopDrawn: lit[0], soundInStop: lit.dropFirst().contains(true))
      }
  }
}
