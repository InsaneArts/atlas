import SwiftUI
import Testing

@testable import AtlasUI

@Suite("WorkingLabel")
struct WorkingLabelTests {
  @Test("a working label crossfades its title and spinner, the same under Reduce Motion")
  func itCrossfadesItsTitleAndSpinner() {
    #expect(WorkingLabel.animation == Motion.textSwap)
    #expect(Motion.Duration.textSwap.seconds <= Motion.Duration.reduced.seconds, "short enough for Reduce Motion")
  }

  @MainActor
  @Test("a working label keeps the width of its title while its spinner shows")
  func itKeepsItsWidthWhileWorking() {
    let resting = ImageRenderer(content: WorkingLabel(title: "Check Again"))
    let working = ImageRenderer(content: WorkingLabel(title: "Check Again", isWorking: true))
    let restingWidth = resting.nsImage?.size.width ?? 0
    let workingWidth = working.nsImage?.size.width ?? 0
    #expect(restingWidth > 0)
    #expect(restingWidth == workingWidth)
  }
}
