import AppKit
import AtlasUI
import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("Probe motion", .serialized, AppKitState())
@MainActor
struct ProbeMotionTests {
  @Test("probe", arguments: ["moment-open", "moment-fold"])
  func probe(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(pose.from, animatesArrivals: false, appearance: Appearance.light)
    try host.render(until: "x") {
      let frame = try host.bitmap()
      return ListMotionFrameTests.textEditFoot(in: frame) != nil
    }
    withAnimation(pose.animation()) {
      host.show(pose.to, animatesArrivals: false)
    }
    host.render()
    var held = TimeInterval.zero
    let dir = URL(fileURLWithPath: ProcessInfo.processInfo.environment["PROBE_DIR"] ?? "/private/tmp/claude-501/-Users-theiskaa-dev-projects-atlas/d8f46b52-ce52-4b25-a7dd-78739ecb8390/scratchpad/probe")
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    for step in pose.steps {
      let moment = pose.moment(at: step)
      while held < moment {
        held = min(held + ViewerMotionPose.tick, moment)
        Motion.Pose.hold(at: held)
        host.render()
      }
      let frame = try host.bitmap()
      try frame.representation(using: .png, properties: [:])?.write(to: dir.appendingPathComponent("\(name)-\(step).png"))
      var rows: [String] = []
      for y in stride(from: 440, through: 560, by: 2) {
        let shades = (286...328).map { ListMotionFrameTests.brightness(of: frame, at: CGPoint(x: $0, y: y)) }
        rows.append("\(y):\(String(format: "%.2f", shades.min() ?? 1))")
      }
      let centre = ListMotionFrameTests.inkCentre(of: frame, in: ListMotionFrameTests.titlePoints)
      print("PROBE \(name) \(step) centre=\(String(describing: centre)) foot=\(String(describing: ListMotionFrameTests.textEditFoot(in: frame))) \(rows.joined(separator: " "))")
    }
  }
}
