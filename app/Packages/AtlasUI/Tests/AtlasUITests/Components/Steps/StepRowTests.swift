import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("StepRow")
struct StepRowTests {
  @Test("a finished step turns its rail green and lights the next step after 120 milliseconds")
  func finishedStepLightsTheNext() throws {
    let pixels = try PanelPixels(
      StepList(
        steps: [
          StepList.Step(title: "Lock and switch", detail: "Choose the Atlas account", state: .done),
          StepList.Step(title: "Atlas opens there by itself", detail: "Choose Set Up Later", state: .current),
          StepList.Step(title: "Allow Atlas twice", detail: "Atlas there shows you how", state: .waiting),
        ]
      )
      .frame(width: OnboardingLayout.column)
    )
    // Each row is 54 points tall: 10 above and below a 17-point title over a 16-point detail, a point apart. The rail
    // runs through the disc's column, centred 25 points in, from the disc's edge, 11 points off the row's middle.
    let column = 25
    let row = 54
    let belowDone = pixels.colour(x: column, y: row - 6)
    let aboveCurrent = pixels.colour(x: column, y: row + 6)
    let belowCurrent = pixels.colour(x: column, y: 2 * row - 6)
    let isGreen = { (colour: (red: Double, green: Double, blue: Double, alpha: Double)) in
      colour.green > colour.red + 0.15 && colour.green > colour.blue + 0.15
    }
    #expect(isGreen(belowDone), "below the finished step: \(belowDone)")
    #expect(isGreen(aboveCurrent), "above the step after it: \(aboveCurrent)")
    #expect(!isGreen(belowCurrent), "below the step still to finish: \(belowCurrent)")

    #expect(StepRow.animation(for: .current, reduceMotion: false) == Motion.state.delay(0.12))
    #expect(StepRow.animation(for: .done, reduceMotion: false) == Motion.state)
    #expect(StepRow.animation(for: .current, reduceMotion: true) == Motion.reduced, "no wait under Reduce Motion")
  }

  @Test("a step with an empty detail draws no second line: its row stands at the least height alone")
  func anEmptyDetailIsHidden() throws {
    let titled = try PanelPixels(
      StepList(
        steps: [
          StepList.Step(title: "Install macOS", detail: "", state: .done),
          StepList.Step(title: "Start the VM", detail: "", state: .current),
        ]
      )
      .frame(width: OnboardingLayout.column)
    )
    let detailed = try PanelPixels(
      StepList(
        steps: [
          StepList.Step(title: "Install macOS", detail: "From Apple", state: .done),
          StepList.Step(title: "Start the VM", detail: "On this Mac", state: .current),
        ]
      )
      .frame(width: OnboardingLayout.column)
    )
    #expect(titled.height == 2 * Int(StepRowLayout.height), "two 48-point rows, no empty line under either title")
    #expect(detailed.height > titled.height, "a detail still takes its line")
  }
}
