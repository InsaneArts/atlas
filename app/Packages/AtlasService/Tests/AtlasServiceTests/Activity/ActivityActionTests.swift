import Testing

@testable import AtlasService

@Suite("ActivityAction")
struct ActivityActionTests {
  @Test(
    "a row's kind comes from the verb its words lead with, done, failed or under way",
    arguments: [
      ("clicked Save in TextEdit", ActivityAction.pointer),
      ("couldn't double-click Ring in Opie", .pointer),
      ("right-clicked in Finder", .pointer),
      ("dragged in Opie", .pointer),
      ("moved the pointer", .pointer),
      ("typed in TextEdit", .keys),
      ("couldn't press ⌘S in TextEdit", .keys),
      ("scrolled in Opie", .scroll),
      ("opened Opie", .window),
      ("opening a link…", .window),
      ("closed the apps", .window),
      ("ran a command", .command),
      ("running a command…", .command),
    ]
  )
  func kindFromVerb(says: String, kind: ActivityAction) {
    #expect(ActivityAction(ActivityLog.Entry(id: "1", says: says, operation: "input")) == kind)
  }

  @Test(
    "installing an app and cleaning up a task's work read as opening and closing do, done, failed or under way",
    arguments: [
      "installed Probe",
      "couldn't install Probe",
      "installing Probe…",
      "cleaned up the task's work",
      "couldn't clean up the task's work",
      "cleaning up…",
    ]
  )
  func kindOfInstallAndCleanUp(says: String) {
    #expect(ActivityAction(ActivityLog.Entry(id: "1", says: says, operation: "run")) == .window)
  }

  @Test("a screenshot and a change of control read by their operation, and words with no known verb by theirs")
  func kindFromOperation() {
    let entry = { (says: String?, operation: String) in
      ActivityAction(ActivityLog.Entry(id: "1", says: says, operation: operation))
    }
    #expect(entry("took a screenshot", "screenshot") == .screenshot)
    #expect(entry("gave back control", "release") == .control)
    #expect(entry("sent 3 actions", "input") == .pointer)
    #expect(entry("used the mouse and keyboard", "input") == .pointer)
    #expect(entry("recovered the helper", "restart") == .command)
    #expect(entry(nil, "action") == .command)
  }
}
