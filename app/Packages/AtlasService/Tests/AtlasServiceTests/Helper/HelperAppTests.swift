import Foundation
import Testing

@testable import AtlasService

@Suite("HelperApp")
struct HelperAppTests {
  @Test("an app with no icon still decodes")
  func noIcon() throws {
    let apps = try ViewerClient.apps(appWithNoIcon, response: reply(200)).apps
    let window = HelperWindow(id: 12, x: 10, y: 40, width: 520, height: 860)
    #expect(apps == [HelperApp(pid: 3, name: "Opie", front: true, windows: [window])])
  }

  @Test("an app's bundle identifier, whether it is hidden, its launch, origin and windows' order read as written")
  func originAndOrder() throws {
    let body = Data(
      #"""
      {"ok": true, "apps": [{"pid": 4, "name": "Probe", "bundleId": "test.atlas.install-probe", "hidden": true,
      "launchedAt": 1792665097600, "origin": {"session": "0123456789ab", "how": "install", "at": 1792665097600},
      "windows": [{"id": 9, "order": 2}]}],
      "sessions": {"0123456789ab": {"name": "Claude Code · understudy", "host": "claude"}}}
      """#.utf8
    )
    let launched = Date(timeIntervalSince1970: 1_792_665_097.6)
    let list = try ViewerClient.apps(body, response: reply(200))
    let app = try #require(list.apps.first)
    #expect(app.bundleId == "test.atlas.install-probe")
    #expect(app.hidden)
    #expect(app.launchedAt == launched)
    #expect(app.origin == HelperAppOrigin(session: "0123456789ab", how: .install, at: launched))
    #expect(app.windows.map(\.order) == [2])
    #expect(list.sessions.keys.sorted() == ["0123456789ab"])
    #expect(list.sessions["0123456789ab"]?.name == "Claude Code · understudy")
  }

  @Test("an app's new fields of the wrong type or missing read as none, and an unknown way of opening as by name")
  func newFieldsReadLeniently() throws {
    let body = Data(
      #"""
      {"ok": true, "apps": [
        {"pid": 4, "name": "Notes", "bundleId": 7, "hidden": "yes", "launchedAt": "soon", "origin": "agent",
        "windows": [{"id": 9, "order": -1}, {"id": 8, "order": "front"}]},
        {"pid": 5, "name": "Probe", "origin": {"how": "someday"}}
      ],
      "sessions": "none"}
      """#.utf8
    )
    let list = try ViewerClient.apps(body, response: reply(200))
    let apps = list.apps
    #expect(apps.count == 2)
    #expect(apps[0].bundleId == nil)
    #expect(!apps[0].hidden)
    #expect(apps[0].launchedAt == nil)
    #expect(apps[0].origin == nil)
    #expect(apps[0].windows.map(\.order) == [nil, nil])
    #expect(apps[1].origin == HelperAppOrigin(session: "", how: .launch))
    #expect(list.sessions.isEmpty)
  }
}
