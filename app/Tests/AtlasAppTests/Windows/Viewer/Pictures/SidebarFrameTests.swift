import AppKit
import AtlasUI
import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("Sidebar frames", .serialized, AppKitState())
@MainActor
struct SidebarFrameTests {
  @Test("collapsing the sidebar never narrows the card in its first frame")
  func collapsingNeverNarrowsTheCard() throws {
    let shown = try ScreenZoomTests.state(.screen)
    var hidden = shown
    hidden.sidebar.mode = .hidden
    // Settled, the hidden sidebar leaves the card the window, so its square picture, centred, moves left.
    #expect(try ViewerHost(hidden).colour(at: Self.leftOfPicture) == .stage)
    #expect(try ViewerHost(shown).colour(at: Self.leftOfPicture) == nil)
    #expect(try ViewerHost(hidden).colour(at: Self.rightOfPicture) == nil)
    // The collapse's first frame still draws the card where it stood beside the sidebar, the picture with it: the card
    // only grows from there, never jumping at once.
    let first = try ViewerHost.firstFrame(from: shown, to: hidden, at: [Self.leftOfPicture, Self.rightOfPicture])
    #expect(first == [nil, .stage])
  }

  @Test("a session moving from Working to Idle keeps its one row on screen in the change's first frame")
  func aMovingRowStaysOnScreen() throws {
    let icon = RowIcon.app(IconImage(id: "moving", image: try Self.blue()))
    let row = ViewerSessionRow(id: "c", icon: icon, name: "Claude Code · understudy", line: "Acting", badge: .acting)
    let other = ViewerSessionRow(id: "x", icon: .tile(.text("X")), name: "Codex", line: "Waiting", badge: .waiting)
    var working = try ScreenZoomTests.state(.screen)
    working.sidebar.sessions = ViewerSessionsState(
      icons: [],
      liveCount: 2,
      groups: [ViewerSessionGroup(.working, rows: [row, other])]
    )
    var idle = working
    idle.sidebar.sessions.groups = [
      ViewerSessionGroup(.working, rows: [other]),
      ViewerSessionGroup(.idle, rows: [ViewerSessionRow(id: "c", icon: icon, name: row.name, line: "Idle")]),
    ]
    let column = (0..<Self.columnSteps)
      .map { CGPoint(x: Self.iconColumn, y: Self.columnTop + CGFloat($0) * Self.columnStep) }
    let host = ViewerHost(working)
    let before = try host.colours(at: column)
    #expect(before.contains(.corner), "the row's blue icon is drawn before the change")
    let first = try ViewerHost.firstFrame(from: working, to: idle, at: column)
    #expect(first.contains(.corner), "the row is on screen in the change's first frame, not gone and back")
  }

  @Test("exactly one pill stands under the chosen row from the first frame of a new choice")
  func exactlyOnePillFromTheFirstFrame() throws {
    let all = PreviewStates.viewerSessions
    var last = all
    let chosen = try #require(all.sidebar.sessions.groups.last?.rows.last)
    last.sidebar.sessions.chosen = .one(chosen.id)
    let before = try Self.pills(in: ViewerHost(all))
    let settled = try Self.pills(in: ViewerHost(last))
    #expect(before.count == 1)
    #expect(settled.count == 1)
    #expect(before != settled, "the pill stood under All sessions before and under the last row once chosen")
    // The new choice's first frame already draws the one pill under the last row: none under All sessions, none on
    // its way between them.
    let first = try ViewerHost.firstFrame(from: all, to: last) { try Self.pills(in: $0) }
    #expect(first == settled)
  }

  @Test("the session rows end above the Library and never reach under it")
  func theRowsEndAboveTheLibrary() throws {
    let host = ViewerHost(PreviewStates.viewerSidebarMany)
    let list = try #require(host.sidebarScrollViews.first, "the sessions scroll in their own room")
    let room = host.frameFromTop(of: list)
    let window = PreviewStates.viewerSize.height
    #expect(room.maxY < window - Self.footGap, "the room ends above the foot")
    // Between the room's lower edge and the Library, the gap a section keeps: only the sidebar's ground, across the
    // rows' width, so no row is drawn there and the Library's heading starts below it.
    let gap = (0..<Int(Self.footGap))
      .map { room.maxY + CGFloat($0) }
    for y in gap {
      let across = Self.wordColumns
        .map { CGPoint(x: $0, y: y) }
      let drawn = try host.brightness(at: across)
      let ground = try host.brightness(at: [CGPoint(x: Self.groundColumn, y: y)])[0]
      #expect(drawn.allSatisfy { abs($0 - ground) <= Self.shadeStep }, "something is drawn at \(y)")
    }
  }

  /// Where the sidebar draws a pill down a column inside the rows' capsules, past their words: the runs of points
  /// whose shade differs from the sidebar's ground beside the capsules, at least a row's capsule high.
  ///
  /// - Parameter host: The window.
  /// - Returns: Each pill's top and bottom, top first.
  /// - Throws: When nothing was drawn.
  static func pills(in host: ViewerHost) throws -> [ClosedRange<CGFloat>] {
    let heights = (0..<columnSteps)
      .map { pillTop + CGFloat($0) * columnStep }
    let inside = try host.brightness(at: heights.map { CGPoint(x: pillColumn, y: $0) })
    let ground = try host.brightness(at: heights.map { CGPoint(x: groundColumn, y: $0) })
    var runs: [ClosedRange<CGFloat>] = []
    var start: CGFloat?
    for (index, height) in heights.enumerated() {
      let isPill = abs(inside[index] - ground[index]) > shadeStep
      if isPill, start == nil { start = height }
      if !isPill || index == heights.count - 1, let top = start {
        if height - top >= leastPill { runs.append(top...height) }
        start = nil
      }
    }
    return runs
  }

  /// 12: the gap a section keeps above the Library, between the sessions' room and the foot.
  static let footGap: CGFloat = 12
  /// Lines down the sidebar from the rows' icons to past their words, from 20 to 200 every 6.
  static let wordColumns = stride(from: CGFloat(20), through: 200, by: 6)
    .map { $0 }

  /// 36: where the column of pills starts, over All sessions' capsule under the top bar.
  static let pillTop: CGFloat = 36
  /// 215: a line down inside the rows' capsules, which run from 10 to 226, past the end of their words.
  static let pillColumn: CGFloat = 215
  /// 231: a line down the sidebar's ground, past the capsules' end and short of the card at 236.
  static let groundColumn: CGFloat = 231
  /// 0.02: how far apart two shades must be to read as the pill against the ground, past rounding.
  static let shadeStep: CGFloat = 0.02
  /// 24: how high a run must be to read as a row's pill, rather than a stray line.
  static let leastPill: CGFloat = 24

  /// A point the square picture covers once the sidebar is hidden, its 790 points centred from 325, but not beside the
  /// whole sidebar, where it starts at 439.
  static let leftOfPicture = CGPoint(x: 380, y: PreviewStates.viewerSize.height / 2)
  /// A point the picture covers beside the whole sidebar, up to 1229, but not once it is hidden, up to 1115.
  static let rightOfPicture = CGPoint(x: 1200, y: PreviewStates.viewerSize.height / 2)
  /// 30: a line down through a session's icon, which stands from 20 to 48 across, clear of its badge.
  static let iconColumn: CGFloat = 30
  /// 60: the column's top, under the top bar.
  static let columnTop: CGFloat = 60
  /// 4: how far apart the column's points stand, finer than an icon.
  static let columnStep: CGFloat = 4
  /// 80: how many points the column reads, down past every row.
  static let columnSteps = 80

  /// A 4-pixel square of the corner's blue, a session's icon the frame read can find.
  ///
  /// - Returns: The image.
  /// - Throws: The failed requirement, when the image cannot be made.
  static func blue() throws -> CGImage {
    let side = 4
    let context = try #require(
      CGContext(
        data: nil,
        width: side,
        height: side,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
      )
    )
    context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: side, height: side))
    return try #require(context.makeImage())
  }
}
