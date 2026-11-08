import AppKit
import AtlasUI
import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("List motion frames", .serialized, AppKitState())
@MainActor
struct ListMotionFrameTests {
  @Test("a run's row in the first frame of its fold stands where it stood, drawn as it was")
  func aFoldingRowStandsWhereItStood() throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == "run-fold" })
    let settled = ViewerHost(pose.from, animatesArrivals: false)
    settled.render()
    let row = try #require(settled.frames[try Self.openedRow(pose)], "the opened run's row is laid out")
    let points = Self.points(in: row)
    let drawn = try settled.brightness(at: points)
    #expect(drawn.contains { $0 < Self.ink }, "the run's words are drawn where the frames are read")
    let first = try ViewerHost.firstFrame(from: pose.from, to: pose.to, animatesArrivals: false) {
      try $0.brightness(at: points)
    }
    // The row is drawn in the same place in the change's first frame: it neither jumps nor is drawn anew.
    #expect(first == drawn)
  }

  @Test(
    "opening or folding a run moves the list in one motion, its row standing still, and settles on the list it shows",
    arguments: ["run-open", "run-fold"]
  )
  func aRunOpensInOneMotion(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let reference = ViewerHost(pose.to, animatesArrivals: false, appearance: Appearance.light)
    reference.render()
    let settled = try reference.pixels(in: Self.list)
    // The disclosure starts in a settled pane: its arrival must not hide the rows being measured.
    let host = ViewerHost(pose.from, animatesArrivals: false, appearance: Appearance.light)
    host.render()
    let row = try #require(host.frames[try Self.openedRow(pose)], "the run's row is laid out")
    let points = Self.points(in: row)
    let still = try host.brightness(at: points)
    let start = try host.pixels(in: Self.list)
    withAnimation(pose.animation()) {
      host.show(pose.to, animatesArrivals: false)
    }
    host.render()
    var held = TimeInterval.zero
    var between = 0
    for step in pose.steps {
      let moment = pose.moment(at: step)
      while held < moment {
        held = min(held + ViewerMotionPose.tick, moment)
        Motion.Pose.hold(at: held)
        host.render()
      }
      #expect(try host.brightness(at: points) == still, "the run's row moved at \(step)")
      let frame = try host.pixels(in: Self.list)
      if frame != start, frame != settled { between += 1 }
    }
    #expect(between > 0, "the list moves through frames between the two, never in one jump")
    Motion.Pose.hold(at: Self.settledAt)
    host.render()
    #expect(try host.pixels(in: Self.list) == settled, "it comes to rest on the list as it shows")
  }

  @Test(
    "held disclosure frames draw identical pixels when repeated in reverse order after the real clock advances",
    arguments: ["run-open", "run-fold"]
  )
  func heldDisclosureFramesDrawIdenticalPixels(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let folder = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-repeated-disclosure-\(UUID().uuidString)")
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    defer {
      try? FileManager.default
        .removeItem(at: folder)
    }
    var frames: [Double: Data] = [:]
    for step in pose.steps {
      #expect(ViewerRender.draw(pose, at: step, to: folder, appearance: Appearance.light))
      frames[step] = try Data(contentsOf: folder.appendingPathComponent("\(pose.file(at: step)).png"))
    }
    RunLoop.main
      .run(until: Date(timeIntervalSinceNow: ViewerMotionPose.tick))
    for step in pose.steps.reversed() {
      #expect(ViewerRender.draw(pose, at: step, to: folder, appearance: Appearance.light))
      let repeated = try Data(contentsOf: folder.appendingPathComponent("\(pose.file(at: step)).png"))
      #expect(repeated == frames[step], "\(name) at \(step) changed pixels")
    }
  }

  /// The layout frame's name of the run a pose opens or folds: the one open on either side of it.
  ///
  /// - Parameter pose: The pose.
  /// - Returns: The run row's frame name.
  /// - Throws: When neither side opens a run.
  static func openedRow(_ pose: ViewerMotionPose) throws -> String {
    let opened = [pose.from, pose.to]
      .compactMap { state -> ViewerActivityState? in
        guard case .activity(let activity) = state.pane else { return nil }
        return activity
      }
      .flatMap { ViewerActivityFixtures.runs($0) }
      .first(where: \.isOpen)
    return "run \(try #require(opened, "the pose opens a run").id)"
  }

  /// Points across a run row's name and the start of what was done there, clear of its disclosure, its icon and its
  /// time, every 2 points across and every point down its middle.
  ///
  /// - Parameter row: The row's frame.
  /// - Returns: The points.
  static func points(in row: CGRect) -> [CGPoint] {
    stride(from: row.midY - Self.band, through: row.midY + Self.band, by: 1)
      .flatMap { y in
        stride(from: row.minX + Self.lead, to: row.minX + Self.reach, by: 2)
          .map { CGPoint(x: $0, y: y) }
      }
  }

  /// The card's list, under its header and clear of the small screen, where the outline draws.
  static let list = CGRect(x: 260, y: 60, width: 840, height: 600)
  /// 48: how far into a run's row its name starts, past its disclosure and icon.
  static let lead: CGFloat = 48
  /// 360: how far into a run's row the points reach, short of its time.
  static let reach: CGFloat = 360
  /// 5: how far above and below a row's middle the points reach, its words' height.
  static let band: CGFloat = 5
  /// 0.5: darker than this is words' ink, not the list's ground.
  static let ink: CGFloat = 0.5
  /// 10 s: long after any disclosure has settled.
  static let settledAt: TimeInterval = 10
}
