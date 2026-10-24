import AtlasCommands
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("SettingsPresenter")
struct SettingsPresenterTests {
  @Test("About shows one control for updates, whatever the update state")
  func aboutShowsOneControlForUpdates() {
    let cases: [(SettingsUpdate, SettingsUpdateButton, SettingsUpdateStatus?)] = [
      (.idle, .check, nil),
      (.checking, .checking, nil),
      (.answered(.upToDate), .check, .done("Atlas is up to date.")),
      (.answered(.newer("0.2.0")), .install, .available("Atlas 0.2.0 is available.")),
      (.answered(.failed), .check, .problem("Atlas couldn't check for updates.")),
      (.answered(.unpublished), .check, .note(SettingsWords.unpublished)),
      (.installing("0.2.0"), .installing, .installing("Installing Atlas 0.2.0…")),
      (.installed, .check, .done(SettingsWords.installed)),
      (.installFailed("Move Atlas.app to /Applications."), .check, .problem("Move Atlas.app to /Applications.")),
    ]
    for (update, button, status) in cases {
      var snapshot = SettingsSnapshot()
      snapshot.session.update = update
      let about = SettingsPresenter.about(snapshot)
      #expect(about.update == button, "\(update)")
      #expect(about.status == status, "\(update)")
    }
  }

  @Test("Check Again shows a spinner in every check's mark while it reads")
  func checkAgainSpinsEveryMark() {
    let items = [
      ReadinessItem(name: "Helper is running", text: "", state: .ready),
      ReadinessItem(name: "Mouse and keyboard", text: "Recover starts them again.", state: .notReady),
    ]
    #expect(SettingsPresenter.ready(items, checking: true).allSatisfy { $0.isChecking })
    #expect(!SettingsPresenter.ready(items).contains { $0.isChecking })
    #expect(SettingsPresenter.ready(items).last?.offersCheck == true)
  }

  @Test("each readiness row carries its place in the list, which staggers its mark's pop, checking or not")
  func eachRowCarriesItsPlace() {
    let items = ["Helper is running", "Mouse and keyboard", "Screen pictures"]
      .map { ReadinessItem(name: $0, text: "", state: .ready) }
    #expect(SettingsPresenter.ready(items).map(\.index) == [0, 1, 2])
    #expect(SettingsPresenter.ready(items, checking: true).map(\.index) == [0, 1, 2])
  }
}
