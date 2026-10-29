import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Viewer pane arrival", .serialized, AppKitState())
struct ViewerPaneArrivalTests {
  @Test("a newly mounted pane hides its scroll indicators before its arrival completes")
  func theFirstArrivalStartsUnsettled() throws {
    var readings: [Bool] = []
    let host = NSHostingView(
      rootView: PaneArrivalProbe { readings.append($0) }
        .modifier(KeptPaneShift(isShown: true, rises: false))
    )
    host.frame = NSRect(x: 0, y: 0, width: 400, height: 300)
    host.layoutSubtreeIfNeeded()
    #expect(try #require(readings.first) == false)
  }
}
