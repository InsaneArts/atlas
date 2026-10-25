import AtlasService
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Apps")
struct ViewerPresenterAppsTests {
  @Test("an app with no windows still lists, with no window rows", arguments: ["", "Find"])
  func appWithNoWindowsLists(query: String) {
    var snapshot = ViewerSnapshot()
    snapshot.panes.apps.apps = [
      HelperApp(pid: 4, name: "Opie", windows: [HelperWindow(id: 1, title: "Starter", width: 1080, height: 590)]),
      HelperApp(pid: 5, name: "Finder"),
    ]
    snapshot.panes.apps.query = query
    let apps = ViewerPresenter.apps(snapshot)
    let finder = apps.apps.first { $0.id == 5 }
    #expect(finder?.windows == [])
    #expect(apps.empty == nil)
  }
}
