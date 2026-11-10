import AsyncSupportTesting
import AtlasUI
import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Card edges", .serialized, AppKitState())
@MainActor
struct CardEdgeTests {
  @Test(
    "the list keeps the card's edges through the sidebar hiding and showing, full screen and a narrow and wide window",
    arguments: [Pane.activity, .screen],
    [ViewerSidebarMode.full, .hidden]
  )
  func theListKeepsTheCardsEdges(pane: Pane, mode: ViewerSidebarMode) async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let start = pane.state(mode: mode)
    let host = ViewerHost(start, animatesArrivals: false)
    await Self.settle(host)
    await Self.expectFresh(host, start, Self.window, "as it opens")
    host.show(pane.state(mode: mode == .hidden ? .full : .hidden))
    await Self.settle(host)
    host.show(start)
    await Self.settle(host)
    await Self.expectFresh(host, start, Self.window, "after the sidebar hid or showed and came back")
    var full = start
    full.isFullScreen = true
    host.resize(to: Self.fullScreen, showing: full)
    await Self.settle(host)
    await Self.expectFresh(host, full, Self.fullScreen, "in full screen")
    host.resize(to: Self.window, showing: start)
    await Self.settle(host)
    await Self.expectFresh(host, start, Self.window, "out of full screen")
    host.resize(to: Self.wide, showing: start)
    await Self.settle(host)
    host.resize(to: Self.narrow, showing: start)
    await Self.settle(host)
    await Self.expectFresh(host, start, Self.narrow, "from wide to narrow")
    host.resize(to: Self.wide, showing: start)
    await Self.settle(host)
    await Self.expectFresh(host, start, Self.wide, "from narrow to wide")
  }

  @Test("the list's leading edge keeps its inset from the header's words after the sidebar hides and shows again")
  func theListLinesUpWithTheHeader() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let shown = Pane.activity
      .state(mode: .full)
    let hidden = Pane.activity
      .state(mode: .hidden)
    let host = ViewerHost(shown, animatesArrivals: false)
    await Self.settle(host)
    host.show(hidden)
    await Self.settle(host)
    host.show(shown)
    await Self.settle(host)
    let header = try #require(host.frames["header words"])
    let filters = try #require(host.frames["activity filters"])
    let leading = ViewerLayout.headerLeading(mode: .full, isFullScreen: false)
    // The card's edge is the header's words less their inset, and the filters stand the list's inset in from it.
    #expect(filters.minX == header.minX - leading + ViewerLayout.swapInset)
  }

  /// A pane the window shows.
  enum Pane: CustomTestStringConvertible {
    case activity
    case screen

    /// The window showing the pane, with the sidebar standing as `mode`.
    ///
    /// - Parameter mode: How the sidebar stands.
    /// - Returns: The state.
    @MainActor func state(mode: ViewerSidebarMode) -> ViewerState {
      var state =
        switch self {
        case .activity: PreviewStates.viewerActivity
        case .screen: PreviewStates.viewerWorking
        }
      state.sidebar.mode = mode
      return state
    }

    var testDescription: String {
      switch self {
      case .activity: "Activity"
      case .screen: "the Screen"
      }
    }
  }

  /// Expects everything in `host` to stand where a window freshly opened at `size` on `state` puts it: every named
  /// element and every scroll view.
  ///
  /// - Parameters:
  ///   - host: The window that went through the changes.
  ///   - state: What it shows.
  ///   - size: Its size.
  ///   - moment: When, for the failure's words.
  private static func expectFresh(_ host: ViewerHost, _ state: ViewerState, _ size: CGSize, _ moment: String) async {
    let fresh = ViewerHost(state, animatesArrivals: false)
    fresh.resize(to: size, showing: state)
    await settle(fresh)
    #expect(host.scrollFrames == fresh.scrollFrames, "the lists stand where they would \(moment)")
    for (id, frame) in fresh.frames {
      #expect(host.frames[id] == frame, "\(id) stands where it would \(moment)")
    }
  }

  /// Lays `host` out and lets its animations and their completions finish, moving the held moment on past each.
  ///
  /// - Parameter host: The window.
  private static func settle(_ host: ViewerHost) async {
    for _ in 0..<Self.turns {
      host.render()
      await afterQueuedWork()
      // An animation that started on this turn, a completion's included, has finished by the next.
      Motion.Pose.hold(at: (Motion.Pose.moment ?? .zero) + Self.settledAfter)
    }
  }

  /// 10: seconds after which every animation has finished.
  private static let settledAfter: TimeInterval = 10
  /// 40: turns enough for a change, the layout it causes and the completions it queues, a slide's included.
  private static let turns = 40
  /// The window's own size.
  private static let window = PreviewStates.viewerSize
  /// A large display's full screen.
  private static let fullScreen = CGSize(width: 2560, height: 1440)
  /// A narrow window, with room for the sidebar beside the card.
  private static let narrow = CGSize(width: 1000, height: 700)
  /// A wide window.
  private static let wide = CGSize(width: 2400, height: 1300)
}
