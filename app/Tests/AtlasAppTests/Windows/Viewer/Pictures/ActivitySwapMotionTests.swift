import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Activity swap motion", .serialized, AppKitState())
@MainActor
struct ActivitySwapMotionTests {
  @Test("a new session choice swaps the list without arrivals")
  func aNewSessionChoiceSwapsTheList() throws {
    try Self.swapsWhole(try Self.pose("session-choose"))
  }

  @Test("a new tab swaps the list without arrivals")
  func aNewTabSwapsTheList() throws {
    try Self.swapsWhole(try Self.pose("filter-choose"))
  }

  @Test("an action arriving in the same list still drops in at the top")
  func anArrivalStillDropsIn() throws {
    let pose = try Self.pose("run-arrive")
    let from = try Self.activity(pose.from)
    let to = try Self.activity(pose.to)
    #expect(from.list == to.list, "the new run arrives in the list already shown")
    let before = Self.runs(from)
    let arrived = try #require(
      Self.runs(to)
        .first { !before.contains($0) },
      "the pose brings a new run"
    )
    let key = "run \(arrived)"
    let settled = try #require(Self.settledFrames(of: pose.to)[key], "the new run is laid out")
    let previous = Motion.Pose.moment
    defer { Motion.Pose.hold(at: previous) }
    let host = Self.settledHost(showing: pose.from)
    host.show(pose.to, animatesArrivals: false)
    host.render()
    Self.advance(host, to: Self.dropping / Motion.playbackRate)
    let dropping = try #require(try Self.inkMiddle(of: settled, in: host), "the new run is drawn as it fades in")
    Self.advance(host, to: Self.settledAt)
    let landed = try #require(try Self.inkMiddle(of: settled, in: host), "the new run is drawn")
    #expect(dropping < landed - Self.drop, "it comes down from above its place")
  }

  @Test("scrolled down, a new session choice brings the list back to its top with nothing waiting")
  func aScrolledSwapShowsTheTop() async throws {
    var activity = ActivityArrivalScrollTests.sessions(40)
    let host = ViewerHost(ActivityArrivalScrollTests.state(activity), animatesArrivals: false)
    await Self.render(host)
    let scroll = try #require(host.scrollViews.first)
    scroll.contentView.scroll(to: CGPoint(x: 0, y: ActivityArrivalScrollTests.scrolled))
    scroll.reflectScrolledClipView(scroll.contentView)
    await Self.render(host)
    #expect(scroll.contentView.bounds.minY > 0, "the list is scrolled down")
    activity.days[0].sessions.insert(ActivityArrivalScrollTests.arrival, at: 0)
    activity.session = .one(SessionSamples.codexKey)
    host.show(ActivityArrivalScrollTests.state(activity))
    await Self.render(host)
    #expect(scroll.contentView.bounds.minY == .zero, "the other session's list shows from its top")
    #expect(host.frames["waiting line"] == nil, "none of its sessions waits as new work")
    #expect(host.frames["session new session"] != nil, "its newest session shows at the top")
  }

  /// Checks that `pose` swaps the list as one piece: the old list fades out, then the new one fades in, each row of the
  /// new list standing in its settled place throughout, nothing waits under a line, and by the swap's end the list
  /// draws exactly what a settled window draws.
  ///
  /// - Parameter pose: A pose that changes the session or the tab.
  /// - Throws: When the pose or a frame is missing.
  static func swapsWhole(_ pose: ViewerMotionPose) throws {
    #expect(try activity(pose.from).list != activity(pose.to).list, "\(pose.name) shows another list")
    let settled = settledFrames(of: pose.to)
    let rows = settled.filter { $0.key.hasPrefix("run ") || $0.key.hasPrefix("session ") }
    try #require(!rows.isEmpty, "the new list has rows")
    let previous = Motion.Pose.moment
    defer { Motion.Pose.hold(at: previous) }
    Motion.Pose.hold(at: .zero)
    let reference = ViewerHost(pose.to, animatesArrivals: false, appearance: Appearance.light)
    reference.render()
    let drawn = try reference.pixels(in: list)
    let landed = try rows.compactMapValues { try inkMiddle(of: $0, in: reference) }
    try #require(!landed.isEmpty, "the new list's rows are drawn")
    let new = try reference.sampled(in: list)
      .ink(in: list)
    let host = settledHost(showing: pose.from)
    let old = try host.sampled(in: list)
      .ink(in: list)
    host.show(pose.to, animatesArrivals: false)
    host.render()
    var inks: [Double: CGFloat] = [:]
    for step in pose.steps {
      let moment = pose.moment(at: step)
      advance(host, to: moment)
      inks[step] = try host.sampled(in: list)
        .ink(in: list)
      if moment > (Motion.Duration.swapOut.seconds + ViewerMotionPose.tick) / Motion.playbackRate {
        // The old list has gone: every row of the new one fades in where it lands, from nowhere else.
        for (key, row) in rows {
          guard let middle = landed[key] else { continue }
          let now = try #require(try inkMiddle(of: row, in: host), "\(key) is drawn at \(step) s")
          #expect(abs(now - middle) <= drawnAlike, "\(key) moved at \(step) s in \(pose.name)")
        }
      }
      #expect(host.frames["waiting line"] == nil, "nothing waits at \(step) s in \(pose.name)")
      if moment >= swapEnd / Motion.playbackRate - ViewerMotionPose.tick {
        #expect(try host.pixels(in: list) == drawn, "\(pose.name) shows the new list whole at \(step) s")
      }
    }
    // The old list fades out where it stands, nothing shows for a moment, then the new list fades in: never the two
    // lists drawn over each other, and never one put in the other's place at once.
    let fading = try #require(inks[clockFadingOut])
    let between = try #require(inks[clockBetween])
    let coming = try #require(inks[clockFadingIn])
    #expect(fading < old - least && fading > between + least, "the old list is half faded at 50 ms")
    #expect(between < min(old, new) - least, "neither list shows at 100 ms")
    #expect(coming > between + least && coming < new - least, "the new list is half faded in at 150 ms")
  }

  /// The pose named `name`.
  ///
  /// - Parameter name: Its name.
  /// - Returns: The pose.
  /// - Throws: When there is none.
  static func pose(_ name: String) throws -> ViewerMotionPose {
    try #require(PreviewStates.viewerMotion.first { $0.name == name })
  }

  /// The Activity `state` shows.
  ///
  /// - Parameter state: What the window shows.
  /// - Returns: The pane.
  /// - Throws: When it shows something else.
  static func activity(_ state: ViewerState) throws -> ViewerActivityState {
    try #require({ if case .activity(let activity) = state.pane { activity } else { nil } }(), "it shows Activity")
  }

  /// Every run's identity in `activity`, newest first.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: The identities.
  static func runs(_ activity: ViewerActivityState) -> [String] {
    activity.days
      .flatMap(\.sessions)
      .flatMap(\.runs)
      .map(\.id)
  }

  /// Where each named element of `state` stands once settled.
  ///
  /// - Parameter state: What the window shows.
  /// - Returns: The frames by name.
  static func settledFrames(of state: ViewerState) -> [String: CGRect] {
    let previous = Motion.Pose.moment
    defer { Motion.Pose.hold(at: previous) }
    Motion.Pose.hold(at: .zero)
    let host = ViewerHost(state, animatesArrivals: false)
    host.render()
    return host.frames
  }

  /// A window showing `state`, every animation it started long over, with the clock held at the change to come.
  ///
  /// - Parameter state: What the window shows first.
  /// - Returns: The window.
  static func settledHost(showing state: ViewerState) -> ViewerHost {
    Motion.Pose.hold(at: -5)
    let host = ViewerHost(state, animatesArrivals: false, appearance: Appearance.light)
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    return host
  }

  /// Moves the held clock on to `moment`, a tick at a time, rendering each tick, as a display draws.
  ///
  /// - Parameters:
  ///   - host: The window.
  ///   - moment: The seconds since the change.
  static func advance(_ host: ViewerHost, to moment: TimeInterval) {
    var held = Motion.Pose.moment ?? .zero
    while held < moment {
      held = min(held + ViewerMotionPose.tick, moment)
      Motion.Pose.hold(at: held)
      host.render()
    }
    host.render()
  }

  /// Where the words of the row standing at `row` sit as `host` draws them: the height of their ink's middle, each
  /// point weighed by how much darker than the ground it is drawn, so a row half faded in sits where it would whole,
  /// and one coming down from above sits higher. The band read reaches ``reach`` above the row, where a drop comes
  /// from, and stops at its foot, clear of what stands under it.
  ///
  /// - Parameters:
  ///   - row: Where the row stands once settled, in the window's points from its top left.
  ///   - host: The window.
  /// - Returns: The middle's height, or nil while nothing is drawn there.
  /// - Throws: When nothing was drawn.
  static func inkMiddle(of row: CGRect, in host: ViewerHost) throws -> CGFloat? {
    let band = CGRect(
      x: row.minX + ListMotionFrameTests.lead,
      y: row.minY - reach,
      width: ListMotionFrameTests.reach - ListMotionFrameTests.lead,
      height: row.height + reach
    )
    let frame = try host.sampled(in: band)
    let points = stride(from: band.minY, to: band.maxY, by: 1)
      .flatMap { y in
        stride(from: band.minX, to: band.maxX, by: 2)
          .map { CGPoint(x: $0, y: y) }
      }
    let brightness = frame.brightness(at: points)
    let ground = brightness.max() ?? 1
    var weight = CGFloat.zero
    var sum = CGFloat.zero
    for (point, bright) in zip(points, brightness) {
      let ink = ground - bright
      weight += ink
      sum += ink * point.y
    }
    return weight > least ? sum / weight : nil
  }

  private static func render(_ host: ViewerHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }

  /// The card's list under the filters, where the outline draws.
  static let list = CGRect(x: 260, y: 90, width: 740, height: 560)
  /// 0.25 s: the old list's fade out and the new one's fade in after it.
  static let swapEnd = Motion.Duration.swapOut.seconds + Motion.Duration.swapIn.seconds
  /// Half way through a new row's fade in, once the rows below have made room for it.
  static let dropping =
    Motion.Duration.swapOut.seconds + Motion.Duration.makeRoom.seconds + Motion.Duration.swapIn.seconds / 2
  /// 50 ms: the old list half way through its fade out.
  static let clockFadingOut = ViewerMotionPose.clockMoments[1]
  /// 100 ms: the old list gone and the new one not yet come.
  static let clockBetween = ViewerMotionPose.clockMoments[2]
  /// 150 ms: the new list part of the way through its fade in.
  static let clockFadingIn = ViewerMotionPose.clockMoments[3]
  /// 2 s: long after any arrival has settled.
  static let settledAt: TimeInterval = 2
  /// 12: how far above a row the ink is read, twice the drop a new row comes down.
  static let reach: CGFloat = 12
  /// 1: how much ink a band holds before anything counts as drawn there.
  static let least: CGFloat = 1
  /// 0.5: how far apart two middles may stand and still be one place, for rounding as words fade.
  static let drawnAlike: CGFloat = 0.5
  /// 1: how far above its place a row half way through its drop at least stands.
  static let drop: CGFloat = 1
}
