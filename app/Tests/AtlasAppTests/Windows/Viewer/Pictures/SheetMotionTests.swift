import AppKit
import AsyncSupportTesting
import AtlasUI
import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Picture sheet motion", .serialized, AppKitState())
@MainActor
struct SheetMotionTests {
  @Test("the sheet grows in its place as it opens and shrinks back as it closes, the window dimming and lighting")
  func theSheetComesAndGoesInPlace() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let shown = PreviewStates.viewerPictureSheet
    var closed = shown
    guard case .activity(var activity) = shown.pane else {
      Issue.record("the sample shows Activity")
      return
    }
    activity.sheet = nil
    closed.pane = .activity(activity)
    let host = ViewerHost(closed, animatesArrivals: false, appearance: Appearance.light)
    host.render()
    var held = TimeInterval.zero
    host.show(shown, animatesArrivals: false)
    let opening = try await Self.follow(host, from: &held)
    host.show(closed, animatesArrivals: false)
    let closing = try await Self.follow(host, from: &held)
    let settled = try #require(opening.last?.frame)
    #expect(host.frames["picture sheet"] != nil)
    for (moves, name) in [(opening, "opening"), (closing, "closing")] {
      let frames = moves.map(\.frame)
      // In its own place: its middle never moves, only its size.
      for frame in frames {
        #expect(abs(frame.midX - settled.midX) <= Self.tolerance, "\(name): it travelled across")
        #expect(abs(frame.midY - settled.midY) <= Self.tolerance, "\(name): it travelled up or down")
        #expect(frame.width >= settled.width * ViewerLayout.sheetEnterScale - Self.tolerance)
      }
      let grows = name == "opening"
      for (earlier, later) in zip(moves, moves.dropFirst()) {
        if grows {
          #expect(later.frame.width >= earlier.frame.width - Self.tolerance, "\(name): it shrank back")
          #expect(later.dimmed <= earlier.dimmed + Self.shade, "\(name): the window lit up again")
        } else {
          #expect(later.frame.width <= earlier.frame.width + Self.tolerance, "\(name): it grew back")
          #expect(later.dimmed >= earlier.dimmed - Self.shade, "\(name): the window dimmed again")
        }
      }
    }
    let lit = try #require(closing.last).dimmed
    let dim = try #require(opening.last).dimmed
    #expect(lit > dim, "the window lights up once it goes")
    let going = try #require(closing.first).frame.width
    #expect(going < settled.width, "it shrinks as it goes")
  }

  @Test("the window behind the sheet dims and comes back on the sheet's own animation, never in one frame")
  func theWindowBehindNeverSnaps() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let shown = PreviewStates.viewerPictureSheet
    var closed = shown
    guard case .activity(var activity) = shown.pane else {
      Issue.record("the sample shows Activity")
      return
    }
    activity.sheet = nil
    closed.pane = .activity(activity)
    let host = ViewerHost(closed, animatesArrivals: false, appearance: Appearance.light)
    host.render()
    var held = TimeInterval.zero
    for (state, name) in [(shown, "opening"), (closed, "closing")] {
      host.show(state, animatesArrivals: false)
      host.render()
      var last = try Self.behind(host)
      let end = held + Self.settledAfter
      while held < end {
        await afterQueuedWork()
        held += Self.sampleEvery
        Motion.Pose.hold(at: held)
        host.render()
        let now = try Self.behind(host)
        for (point, (was, is_)) in zip(Self.behindPoints, zip(last, now)) {
          #expect(abs(is_ - was) <= Self.step, "\(name): \(point) changed \(abs(is_ - was)) in one frame")
        }
        last = now
      }
    }
  }

  /// How bright the window draws each of ``behindPoints``.
  ///
  /// - Parameter host: The window.
  /// - Returns: Each point's brightness.
  /// - Throws: When a frame cannot be drawn.
  static func behind(_ host: ViewerHost) throws -> [CGFloat] {
    try behindPoints.map { point in
      try host.sampled(in: CGRect(origin: point, size: spot))
        .brightness(at: point)
    }
  }

  /// One sampled frame of the sheet's move: where it stands and how bright the window behind it is.
  struct Move {
    /// The sheet's frame.
    let frame: CGRect
    /// How bright the window is beside the sheet.
    let dimmed: CGFloat
  }

  /// Follows the sheet through its move, a frame each sampled moment, until it has settled.
  ///
  /// - Parameters:
  ///   - host: The window.
  ///   - held: The moment held, which moves on.
  /// - Returns: Each frame's move.
  /// - Throws: When a frame cannot be drawn.
  static func follow(_ host: ViewerHost, from held: inout TimeInterval) async throws -> [Move] {
    host.render()
    let end = held + Self.settledAfter
    var moves: [Move] = []
    while held < end {
      // Each frame lets the main actor's other work run, so suites beside this one are never starved of it.
      await afterQueuedWork()
      held += Self.sampleEvery
      Motion.Pose.hold(at: held)
      host.render()
      guard let frame = host.frames["picture sheet"] else { continue }
      let beside = try host.sampled(in: CGRect(origin: Self.beside, size: Self.spot))
      moves.append(Move(frame: frame, dimmed: beside.brightness(at: Self.beside)))
    }
    return moves
  }

  /// 1/60 s: how often the move is sampled.
  static let sampleEvery: TimeInterval = 1.0 / 60
  /// 0.5 s: long after the sheet has settled.
  static let settledAfter: TimeInterval = 0.5
  /// 0.5 pt: how far the sheet's edges may wobble from one frame to the next.
  static let tolerance: CGFloat = 0.5
  /// 0.01: how much the window's brightness may wobble from one frame to the next.
  static let shade: CGFloat = 0.01
  /// A point in the sidebar, clear of the sheet, where the scrim dims the window.
  static let beside = CGPoint(x: 100, y: 600)
  /// The part of the frame read around it.
  static let spot = CGSize(width: 2, height: 2)
  /// Points of the window behind the sheet, clear of it and its shadow: a sidebar row, the sidebar's ground, the card's
  /// header, a time in the list and the footer's control.
  static let behindPoints = [
    CGPoint(x: 60, y: 106),
    CGPoint(x: 100, y: 600),
    CGPoint(x: 300, y: 27),
    CGPoint(x: 1386, y: 112),
    CGPoint(x: 1300, y: 856),
  ]
  /// 0.12: the most a point behind the sheet may change in one frame of its fade.
  static let step: CGFloat = 0.12
}
