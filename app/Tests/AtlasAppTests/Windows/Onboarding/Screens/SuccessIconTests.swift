import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

/// Here, not in AtlasUI's tests, because an arrival is a view appearing in a window, which AtlasUI never imports.
@MainActor
@Suite("SuccessIcon")
struct SuccessIconTests {
  @Test("the done screen's icon settles once per arrival, not on every redraw")
  func iconSettlesOncePerArrival() {
    let plays = PlayCount()
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: OnboardingLayout.card),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    let host = NSHostingView(rootView: Arrival(number: 1, plays: plays))
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    #expect(plays.count == 1, "it plays as it arrives")
    for redraw in 0..<3 {
      host.rootView = Arrival(number: 1, plays: plays, redraw: redraw)
      host.layoutSubtreeIfNeeded()
    }
    #expect(plays.count == 1, "a redraw never plays it again")
    host.rootView = Arrival(number: 2, plays: plays)
    host.layoutSubtreeIfNeeded()
    #expect(plays.count == 2, "a new arrival plays it once more")
  }
}

/// How many times the moment played.
@MainActor
private final class PlayCount {
  /// The plays so far.
  var count = 0
}

/// The done screen's hero, one arrival per number, redrawn with a changing caption beside it.
private struct Arrival: View {
  /// Which arrival this is: a new number is a new identity.
  let number: Int
  /// Counts the plays.
  let plays: PlayCount
  /// A value that changes on each redraw.
  var redraw = 0

  var body: some View {
    VStack {
      SuccessIcon(tile: OnboardingLayout.doneTile) { plays.count += 1 }
        .id(number)
      Text(verbatim: "\(redraw)")
    }
  }
}
