import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("PicturesSummary")
struct PicturesSummaryTests {
  @Test(
    "pictures read as on for seven days with typed text whole unless the settings say otherwise",
    arguments: [
      (#"{"ok": true, "config": {"helper_uid": 502}}"#, PicturesSettings()),
      (
        #"{"ok": true, "config": {"activity": {"pictures_before_actions": false, "keep_days": 3}}}"#,
        PicturesSettings(isOn: false, keepDays: 3)
      ),
      (
        #"{"ok": true, "config": {"activity": {"keep_days": 21, "typed_text_limit": 80}}}"#,
        PicturesSettings(keepDays: 21, typedTextLimit: 80)
      ),
      (
        #"{"ok": true, "config": {"activity": {"keep_days": 0, "typed_text_limit": 0}}}"#,
        PicturesSettings()
      ),
      (
        #"{"ok": true, "config": {"activity": {"keep_days": "a week", "pictures_before_actions": "no"}}}"#,
        PicturesSettings()
      ),
    ] as [(String, PicturesSettings)]
  )
  func readsTheSettings(reply: String, settings: PicturesSettings) throws {
    let outcome = Outcome.parse(status: 0, stdout: Data(reply.utf8), stderr: Data())
    #expect(try #require(PicturesSummary.of(outcome)).settings == settings)
  }

  @Test("how many pictures this Mac keeps and their size read from the reply, and none when it says nothing")
  func readsTheCount() throws {
    let counted = Outcome.parse(
      status: 0,
      stdout: Data(#"{"ok": true, "config": {}, "pictures": {"count": 1240, "bytes": 148897792}}"#.utf8),
      stderr: Data()
    )
    let summary = try #require(PicturesSummary.of(counted))
    #expect(summary.count == 1240)
    #expect(summary.bytes == 148_897_792)
    let silent = Outcome.parse(status: 0, stdout: Data(#"{"ok": true, "config": {}}"#.utf8), stderr: Data())
    #expect(PicturesSummary.of(silent)?.count == 0)
  }

  @Test("a reply without settings, or a failure, reads as unknown")
  func unknownWithoutSettings() {
    for (status, reply) in [
      (Int32(0), #"{"ok": true}"#),
      (1, #"{"ok": false, "code": "config", "error": "Atlas is not set up yet"}"#),
    ] {
      let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
      #expect(PicturesSummary.of(outcome) == nil)
    }
  }
}
