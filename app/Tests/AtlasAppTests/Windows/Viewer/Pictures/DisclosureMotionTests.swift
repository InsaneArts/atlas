import AppKit
import AtlasUI
import CoreGraphics
import Foundation
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Disclosure motion", .serialized, AppKitState())
@MainActor
struct DisclosureMotionTests {
  @Test(
    "opening or folding is one move: its row stands still, the rows below move one way, and its content never vanishes",
    arguments: ["run-open", "run-fold", "session-open", "session-fold"]
  )
  func aDisclosureIsOneMove(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(pose.from, animatesArrivals: false, appearance: Appearance.light)
    host.render()
    let row = try #require(host.frames[try Self.toggledRow(pose)], "the toggled row is laid out")
    let points = Self.rowPoints(row)
    let still = try host.brightness(at: points)
    let band = CGRect(x: row.minX + Self.contentLead, y: row.maxY, width: Self.contentReach, height: Self.contentDepth)
    let start = try host.sampled()
    let opens = Self.opens(pose)
    withAnimation(pose.animation()) {
      host.show(pose.to, animatesArrivals: false)
    }
    host.render()
    var held = TimeInterval.zero
    var below: [CGFloat] = []
    var ink: [CGFloat] = [start.ink(in: band)]
    while held < Self.settledAt {
      held += Self.sampleEvery
      Motion.Pose.hold(at: held)
      host.render()
      #expect(try host.brightness(at: points) == still, "\(name): the row moved at \(held) s")
      let frame = try host.sampled()
      ink.append(frame.ink(in: band))
      let columns = (row.minX + Self.sessionColumn.lowerBound)...(row.minX + Self.sessionColumn.upperBound)
      if let top = frame.firstInk(across: columns, from: row.maxY + 1, to: Self.listBottom, ink: Self.inkLevel) {
        below.append(top)
      }
    }
    // The rows below only ever move away from the row as it opens, and towards it as it folds.
    for (earlier, later) in zip(below, below.dropFirst()) {
      if opens {
        #expect(later >= earlier - Self.tolerance, "\(name): the rows below moved back up")
      } else {
        #expect(later <= earlier + Self.tolerance, "\(name): the rows below moved back down")
      }
    }
    // What the row holds and its key items fade into one another: under the row there is always something drawn.
    let least = try #require(ink.min())
    let ends = min(try #require(ink.first), try #require(ink.last))
    #expect(least >= ends * Self.leastShare, "\(name): the content under the row vanished, \(least) of \(ends)")
  }

  /// Whether `pose` opens what it toggles, rather than folding it.
  ///
  /// - Parameter pose: The pose.
  /// - Returns: Whether it opens.
  static func opens(_ pose: ViewerMotionPose) -> Bool {
    pose.name.contains("open")
  }

  /// The layout frame's name of the session or run `pose` opens or folds.
  ///
  /// - Parameter pose: The pose.
  /// - Returns: The row's frame name.
  /// - Throws: When the pose toggles neither.
  static func toggledRow(_ pose: ViewerMotionPose) throws -> String {
    let sides = [pose.from, pose.to]
      .compactMap { state -> ViewerActivityState? in
        guard case .activity(let activity) = state.pane else { return nil }
        return activity
      }
    try #require(sides.count == 2, "the pose shows Activity on both sides")
    let sessions = sides.map { $0.days.flatMap(\.sessions) }
    if let session = zip(sessions[0], sessions[1]).first(where: { $0.isOpen != $1.isOpen })?.0 {
      return "session \(session.id)"
    }
    let runs = sides.map { ViewerActivityFixtures.runs($0) }
    let run = try #require(zip(runs[0], runs[1]).first { $0.isOpen != $1.isOpen }?.0, "the pose toggles a run")
    return "run \(run.id)"
  }

  /// Points across a row's words, clear of its disclosure, its lead and its time, every 2 points across and every point
  /// down its middle.
  ///
  /// - Parameter row: The row's frame.
  /// - Returns: The points.
  static func rowPoints(_ row: CGRect) -> [CGPoint] {
    stride(from: row.midY - Self.wordsBand, through: row.midY + Self.wordsBand, by: 1)
      .flatMap { y in
        stride(from: row.minX + Self.contentLead, to: row.minX + Self.contentReach, by: 2)
          .map { CGPoint(x: $0, y: y) }
      }
  }

  /// 1/60 s: how often the move is sampled.
  static let sampleEvery: TimeInterval = 1.0 / 60
  /// 0.6 s: long after a disclosure has settled.
  static let settledAt: TimeInterval = 0.6
  /// 66: where a session's words start in its row, past its disclosure and lead.
  static let contentLead: CGFloat = 66
  /// 400: how far across the content under a row is read.
  static let contentReach: CGFloat = 400
  /// 44: how far down the content under a row is read: a session's row, or a run's problem line and more.
  static let contentDepth: CGFloat = 44
  /// 12 to 24: the session chevrons' and the day headings' column, which the rows below are followed by.
  static let sessionColumn: ClosedRange<CGFloat> = 12...24
  /// 660: where the list ends, above the small screen.
  static let listBottom: CGFloat = 660
  /// 0.5: darker than this is ink, not the list's ground.
  static let inkLevel: CGFloat = 0.5
  /// 0.5 pt: how far the rows below may wobble from one frame to the next as they settle.
  static let tolerance: CGFloat = 0.5
  /// 0.3: the least share of the ink under the row, against its two ends, any frame keeps.
  static let leastShare: CGFloat = 0.3
  /// 5: how far above and below a row's middle its words reach.
  static let wordsBand: CGFloat = 5
}
