import Foundation
import Testing

@testable import AtlasKit

@Suite("HelperApp")
struct HelperAppTests {
  @Test("an app with no icon still decodes")
  func noIcon() throws {
    let apps = try ViewerClient.apps(appWithNoIcon, response: reply(200))
    let window = HelperWindow(id: 12, x: 10, y: 40, width: 520, height: 860)
    #expect(apps == [HelperApp(pid: 3, name: "Opie", front: true, windows: [window])])
  }
}
