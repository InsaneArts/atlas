import Testing

@testable import AtlasService

@Suite("Viewed desktop")
struct ViewedDesktopTests {
  @Test("a reply is dropped only once another desktop is viewed than the one its request named")
  func onlyAnotherDesktopDropsAReply() throws {
    let viewed = ViewedDesktop()
    try viewed.still(nil)
    viewed.view("a1")
    try viewed.still(nil)
    try viewed.still("a1")
    viewed.view("b2")
    #expect(throws: ServiceError.otherDesktop) { try viewed.still("a1") }
    #expect(viewed.id == "b2")
  }
}
