import AppKit
import AsyncSupportTesting
import AtlasUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("SettingsDock", .serialized, AppKitState())
struct SettingsDockTests {
  @Test("the first move into the dock glides, and following a short move of System Settings moves at once")
  func theFirstMoveGlidesAndFollowingIsInstant() async throws {
    let docked = try SettingsDockFixture()
    defer { docked.close() }
    #expect(docked.glides.travels.count == 1, "the window glides into the dock")
    #expect(docked.glides.contentFades == [0, 1], "its content fades around the glide")
    let first = try #require(docked.glides.travels.first)
    #expect(first.origin == docked.place)

    // The person moves System Settings a little: at the next look the window is beside it again, set there.
    let before = docked.place
    docked.settings.frame = docked.settings.frame.offsetBy(dx: 12, dy: -10)
    try #require(docked.place == CGPoint(x: before.x + 12, y: before.y - 10), "a short move stays on the same side")
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.clock.advance(by: SettingsDock.followEvery)
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock looked and waits again")
    #expect(docked.window.frame.origin == docked.place)
    #expect(docked.glides.travels.count == 1, "following never glides")
  }

  @Test("while the person drags System Settings, the panel follows it every frame, not every eighth of a second")
  func draggingIsFollowedEveryFrame() async throws {
    let docked = try SettingsDockFixture()
    defer { docked.close() }
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.pointer.isDown = true
    docked.clock.advance(by: SettingsDock.followEvery)
    for step in 1...SettingsDockFixture.dragFrames {
      try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock never looked a frame later")
      let before = docked.place
      docked.settings.frame = docked.settings.frame.offsetBy(dx: SettingsDockFixture.dragStep, dy: 0)
      try #require(
        docked.place == CGPoint(x: before.x + SettingsDockFixture.dragStep, y: before.y),
        "frame \(step) asks for one short following move"
      )
      docked.clock.advance(by: SettingsDock.followFrame)
      try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock stopped following")
      #expect(docked.window.frame.origin == docked.place, "frame \(step) of the drag")
    }
  }

  @Test(
    "after a jump, the panel moves at once under Reduce Motion and otherwise catches up on a spring",
    arguments: [false, true]
  )
  func aJumpRespectsReduceMotion(reduced: Bool) async throws {
    let docked = try SettingsDockFixture(reduceMotion: { reduced })
    defer { docked.close() }
    let start = docked.window.frame.origin
    docked.settings.frame = docked.settings.frame.offsetBy(dx: 0, dy: -200)
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.clock.advance(by: SettingsDock.followEvery)
    if reduced {
      try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock never resumed its usual looks")
      #expect(docked.window.frame.origin == docked.place)
      #expect(docked.glides.travels.count == 1, "following moves without a glide")
      return
    }
    try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock never caught up frame by frame")
    let after = docked.window.frame.origin
    #expect(after.y < start.y && after.y > docked.place.y, "part of the way, not snapped: \(after)")
    var last = after.y
    for _ in 0..<60 where docked.window.frame.origin != docked.place {
      docked.clock.advance(by: SettingsDock.followFrame)
      try await docked.clock.untilSleeping(1, for: nil, "the dock stopped")
      #expect(docked.window.frame.origin.y <= last, "it never turns back, as a bounce would")
      #expect(docked.window.frame.origin.y >= docked.place.y, "it never overshoots")
      last = docked.window.frame.origin.y
    }
    #expect(docked.window.frame.origin == docked.place, "it lands in its place")
  }

  @Test("a short move is taken at once, a long one on the spring until it lands, and the spring never overshoots")
  func followingRule() {
    var follow = DockFollow()
    let target = CGPoint(x: 100, y: 100)
    #expect(follow.next(from: CGPoint(x: 90, y: 100), toward: target, after: 1.0 / 60) == target)
    let first = follow.next(from: CGPoint(x: 100, y: 300), toward: target, after: 1.0 / 60)
    #expect(first.y < 300 && first.y > 100 && follow.isCatchingUp)
    var point = first
    for _ in 0..<60 where follow.isCatchingUp {
      let next = follow.next(from: point, toward: target, after: 1.0 / 60)
      #expect(next.y <= point.y && next.y >= target.y)
      point = next
    }
    #expect(point == target && !follow.isCatchingUp)
  }
}
