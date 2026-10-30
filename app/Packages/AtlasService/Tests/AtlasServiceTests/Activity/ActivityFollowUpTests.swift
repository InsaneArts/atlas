import Testing

@testable import AtlasService

@Suite("Activity follow-up identity")
struct ActivityFollowUpTests {
  @Test("only the same recorded shortcut in the same app establishes a retry")
  func shortcutsKeepTheirIdentity() {
    let save = ActivityLog.Entry(id: "save", verb: "key", app: "Notes", label: "⌘S", keys: [["⌘", "S"]])
    var next = save
    next.id = "next"
    #expect(ActivityFollowUp.retries(next, save))
    next.keys = [["⌘", "W"]]
    #expect(!ActivityFollowUp.retries(next, save))
    next.label = "⌘W"
    #expect(!ActivityFollowUp.retries(next, save))
    next = save
    next.app = "Mail"
    #expect(!ActivityFollowUp.retries(next, save))
  }

  @Test("different targets and missing action identity use neutral follow-up wording")
  func targetsAndMissingEvidenceAreNeutral() {
    let save = ActivityLog.Entry(id: "save", verb: "click", app: "Notes", label: "Save")
    var next = save
    next.label = "Cancel"
    #expect(!ActivityFollowUp.retries(next, save))
    next.label = nil
    #expect(!ActivityFollowUp.retries(next, save))
    for verb in [nil, "click", "type", "scroll", "run"] as [String?] {
      let unknown = ActivityLog.Entry(id: "unknown", verb: verb, app: "Notes")
      #expect(!ActivityFollowUp.retries(unknown, unknown))
    }
  }
}
