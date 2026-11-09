import Testing

@testable import AtlasUI

@Suite("What VoiceOver reads of an allowance")
struct ViewerAllowanceLabelTests {
  /// The person allowed the rest of the task.
  static let badge = AllowanceBadge(words: "You allowed for this task", tone: .accent)

  @Test("a step reads what let it act after its words, and one with no badge reads as before")
  func aStepReadsItsAllowance() {
    var step = ViewerActivityStep(
      id: "c1",
      kind: .pointer,
      words: ViewerStepWords("Clicked", target: "Merge"),
      time: "02:12",
      account: "github.com",
      allowance: Self.badge
    )
    #expect(step.label == "Clicked Merge, You allowed for this task, 02:12")
    step.allowance = nil
    #expect(step.label == "Clicked Merge, 02:12")
  }

  @Test("a run reads the account it acted in, once even when titled by it, and what let it act there")
  func aRunReadsItsAccountAndAllowance() {
    var run = ViewerOutlineRun(
      id: "run",
      icon: .you,
      name: "github.com",
      summary: "Clicked · 1 action",
      time: "02:12",
      account: "github.com",
      allowance: AllowanceBadge(words: "Trusted", tone: .quiet)
    )
    #expect(run.label == "github.com, Clicked · 1 action, Trusted, 02:12")
    run.name = "Google Chrome"
    #expect(run.label == "Google Chrome, github.com, Clicked · 1 action, Trusted, 02:12")
  }
}
