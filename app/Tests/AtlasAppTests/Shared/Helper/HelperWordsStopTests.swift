import AtlasService
import Testing

@testable import AtlasApp

@Suite("HelperWords+Stop")
struct HelperWordsStopTests {
  /// Each desktop's title by its name in the settings, as the desktops list them.
  static let titles = ["second-login": "Second Login", "work": "work", "vm-2": "VM 2"]

  /// A Stop of every desktop, each named where it landed.
  ///
  /// - Parameters:
  ///   - stopped: The desktops it held for certain.
  ///   - failed: The desktops it failed on.
  ///   - unsure: The desktops that did not confirm it.
  ///   - warnings: What the service warned of.
  /// - Returns: The outcome.
  static func outcome(
    stopped: [String],
    failed: [String] = [],
    unsure: [String] = [],
    warnings: [String] = []
  ) -> StopOutcome {
    var outcome = StopOutcome(warnings: warnings)
    outcome.held = failed.isEmpty
    outcome.stopped = stopped
    outcome.failed = failed
    outcome.uncertain = failed + unsure
    return outcome
  }

  @Test("an uncertain desktop is named in Stop's words by its title, beside the ones it stopped")
  func anUncertainDesktopIsNamed() {
    let outcome = Self.outcome(
      stopped: ["second-login"],
      unsure: ["work"],
      warnings: ["work has not confirmed this Stop. Agents are stopped here all the same."]
    )
    #expect(
      HelperWords.stopped(outcome, titles: Self.titles)
        == "Agents stopped on Second Login, but Atlas isn't sure they stopped on work. work has not confirmed this "
        + "Stop. Agents are stopped here all the same."
    )
    #expect(HelperWords.stopSaysMore(outcome))
    let none = Self.outcome(stopped: [], unsure: ["second-login", "vm-2"])
    let words = HelperWords.stopped(none, titles: Self.titles)
    #expect(words == "Atlas isn't sure agents stopped on Second Login and VM 2.")
  }

  @Test("a failed desktop is named by its title, and one only unsure beside it is named too")
  func aFailedDesktopIsNamed() {
    let failed = Self.outcome(stopped: ["second-login"], failed: ["work"])
    #expect(
      HelperWords.stopped(failed, titles: Self.titles)
        == "Agents stopped on Second Login, but Atlas couldn't stop them on work."
    )
    let both = Self.outcome(stopped: ["second-login"], failed: ["work"], unsure: ["vm-2"])
    #expect(
      HelperWords.stopped(both, titles: Self.titles)
        == "Agents stopped on Second Login, but Atlas couldn't stop them on work. Atlas isn't sure they stopped on VM 2."
    )
    #expect(HelperWords.stopped(failed) == "Agents stopped on second-login, but Atlas couldn't stop them on work.")
  }

  @Test("with one desktop a Stop says what it always said")
  func withOneDesktopAStopIsUnchanged() {
    #expect(HelperWords.stopped(StopOutcome()) == "Agents stopped.")
    #expect(!HelperWords.stopSaysMore(StopOutcome()))
    let warned = StopOutcome(warnings: ["The helper could not be reached"])
    #expect(HelperWords.stopped(warned, titles: Self.titles) == "Agents stopped. The helper could not be reached.")
  }
}
