import Testing

@testable import AtlasKit

@Suite("Permission+Guide")
struct PermissionGuideTests {
  @Test("the guide stays on the permissions step until its switch is on, and steps aside on any other step")
  func guideStays() throws {
    let off = try asker(installed: true, running: true, ax: false, screen: false).asker
    let on = try asker(installed: true, running: true, ax: true, screen: false).asker
    #expect(Permission.accessibility.guideStays(on: .permissions, asker: off))
    #expect(!Permission.accessibility.guideStays(on: .permissions, asker: on))
    #expect(!Permission.accessibility.guideStays(on: .helperDone, asker: off))
  }
}
