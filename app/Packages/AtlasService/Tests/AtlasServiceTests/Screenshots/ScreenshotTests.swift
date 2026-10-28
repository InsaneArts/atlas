import Foundation
import Testing

@testable import AtlasService

@Suite("Screenshot")
struct ScreenshotTests {
  @Test("a screenshot's time is read as milliseconds since 1970, and one whose size did not read still decodes")
  func takenAt() throws {
    let shots = try ViewerClient.screenshots(screenshotWithNoSize, response: reply(200))
    let taken = Date(timeIntervalSince1970: 1_700_000_005)
    #expect(shots.shots == [Screenshot(name: "1700000005000000000-capture.png", takenAt: taken)])
  }

  @Test("the screenshots' folder counts only as an absolute path, and only a listed name inside it is a file")
  func folder() throws {
    let name = "1700000005000000000-capture.png"
    let listed = try ViewerClient.screenshots(
      Data(#"{"ok": true, "folder": "/Users/me/runs", "screenshots": [{"name": "\#(name)"}]}"#.utf8),
      response: reply(200)
    )
    #expect(listed.file(named: name)?.path == "/Users/me/runs/\(name)")
    #expect(listed.file(named: "1700000009000000000-capture.png") == nil, "a name the list does not hold")
    let relative = try ViewerClient.screenshots(
      Data(#"{"ok": true, "folder": "runs", "screenshots": [{"name": "\#(name)"}]}"#.utf8),
      response: reply(200)
    )
    #expect(relative.folder == nil)
    #expect(relative.file(named: name) == nil)
  }

  @Test("a capture's time reads from its file name, and a name of another shape has none")
  func aCapturesTimeReadsFromItsName() {
    #expect(Screenshot.takenAt(named: "1793163300000000000-capture.png") == Date(timeIntervalSince1970: 1_793_163_300))
    #expect(Screenshot.takenAt(named: "capture.png") == nil)
    #expect(Screenshot.takenAt(named: "1793163300000000000.png") == nil)
  }
}
