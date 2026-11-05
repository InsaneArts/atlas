import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@MainActor
@Suite("SettingsPictures")
struct SettingsPicturesTests {
  /// A summary of pictures kept 3 days, typed text up to 120 characters, with 1,240 pictures on this Mac.
  static let summary =
    #"{"ok": true, "config": {"activity": {"pictures_before_actions": false, "keep_days": 3, "typed_text_limit": 120}}, "pictures": {"count": 1240, "bytes": 148897792}}"#
  /// What a write answers.
  static let written = #"{"ok": true, "config": {"helper_uid": 502}}"#

  @Test("pictures read from Atlas's settings with how many this Mac holds, on for seven days until read")
  func picturesRead() async {
    let commands = ScriptedCommands(["config --pictures-summary": Self.summary])
    let controller = SettingsController.scripted(commands: commands)
    let unread = controller.state.general
    #expect(unread.picturesBeforeActions && unread.keepFor == .sevenDays && unread.typedText == .full)
    #expect(!unread.canDeletePictures)
    await controller.readPictures()
    let general = controller.state.general
    #expect(!general.picturesBeforeActions)
    #expect(general.keepFor == .threeDays)
    #expect(general.typedText == .limited)
    #expect(general.typedTextLimit == 120)
    #expect(general.storedPictures.hasPrefix("\(1240.formatted()) pictures · "))
    #expect(general.canDeletePictures)
  }

  @Test("each choice writes its own setting: the switch, the days, a custom number, and the typed text's limit")
  func choicesWrite() async throws {
    let commands = ScriptedCommands(["config": Self.written])
    let controller = SettingsController.scripted(commands: commands)
    var general = controller.state.general
    general.picturesBeforeActions = false
    controller.update(general)
    try await settle("the switch was never written") { commands.ran.count == 1 }

    general = controller.state.general
    general.keepFor = .threeDays
    controller.update(general)
    try await settle("the days were never written") { commands.ran.count == 2 }

    general = controller.state.general
    general.keepFor = .custom
    controller.update(general)
    #expect(controller.state.general.keepFor == .custom, "Custom… shows the days though they did not change")
    general = controller.state.general
    general.keepDays = 14
    controller.update(general)
    try await settle("the custom days were never written") { commands.ran.count == 3 }

    general = controller.state.general
    general.typedText = .limited
    controller.update(general)
    try await settle("the limit was never written") { commands.ran.count == 4 }

    general = controller.state.general
    general.typedText = .full
    controller.update(general)
    try await settle("the limit was never lifted") { commands.ran.count == 5 }

    #expect(
      commands.ran == [
        Action.keepPictures(false).arguments,
        Action.keepPicturesFor(days: 3).arguments,
        Action.keepPicturesFor(days: 14).arguments,
        Action.limitTypedText(SettingsGeneralState.typedTextDefault).arguments,
        Action.limitTypedText(0).arguments,
      ]
    )
    #expect(controller.state.general.keepDays == 14)
    #expect(controller.state.general.keepFor == .custom)
  }

  @Test("a limit of 1 character in Atlas's settings stays as it is when another picture setting changes")
  func untouchedLimitKept() async throws {
    let summary = Self.summary
      .replacingOccurrences(of: #""typed_text_limit": 120"#, with: #""typed_text_limit": 1"#)
    let commands = ScriptedCommands(["config --pictures-summary": summary, "config": Self.written])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readPictures()
    #expect(controller.state.general.typedTextLimit == 1)
    var general = controller.state.general
    general.keepFor = .sevenDays
    controller.update(general)
    try await settle("the days were never written") { commands.ran.count == 2 }
    #expect(commands.ran.last == Action.keepPicturesFor(days: 7).arguments, "only the days are written")
    #expect(controller.pictures.shown.typedTextLimit == 1, "the limit is never rewritten")
  }

  @Test("a picture setting that could not be written says why and keeps what the settings last confirmed")
  func aFailedWriteSaysWhy() async throws {
    let commands = ScriptedCommands([
      "config --pictures-before-actions":
        #"{"ok": false, "code": "config", "error": "~/.config/atlas.toml changed while Atlas was updating it"}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.choosePictures(PicturesSettings(isOn: false))
    #expect(commands.ran == [Action.keepPictures(false).arguments])
    #expect(controller.state.general.picturesBeforeActions)
    let notice = try #require(controller.state.general.notice)
    #expect(notice.kind == .problem)
    #expect(notice.text.contains("changed while Atlas was updating it"))
  }

  @Test("choices made while a picture setting is written are written once it ends, one write at a time")
  func writesOneAtATime() async throws {
    let commands = ScriptedCommands(["config": Self.written, "config --pictures-summary": Self.summary])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let controller = SettingsController.scripted(commands: commands)
    let first = Task { await controller.choosePictures(PicturesSettings(isOn: false)) }
    try await settle("the first write never started") { commands.ran.count == 1 }
    await controller.choosePictures(PicturesSettings(isOn: true))
    await controller.choosePictures(PicturesSettings(isOn: false, keepDays: 3))
    let reading = Task { await controller.readPictures() }
    try await settle("the reading never started") { commands.ran.count == 2 }
    gate.open()
    await first.value
    await reading.value
    #expect(
      commands.ran == [
        Action.keepPictures(false).arguments, Action.picturesSummary.arguments,
        Action.keepPicturesFor(days: 3).arguments,
      ]
    )
    #expect(controller.pictures.confirmed == PicturesSettings(isOn: false, keepDays: 3))
    #expect(controller.state.general.keepFor == .threeDays, "the reading under a write is not kept over it")
  }

  @Test("a reading begun before the person's choice never turns it back")
  func aStaleReadingIsDropped() async throws {
    let commands = ScriptedCommands(["config": Self.written, "config --pictures-summary": Self.summary])
    let gate = ReleaseSignal()
    commands.whileRunning { arguments in
      if arguments == Action.picturesSummary.arguments { await gate.wait() }
    }
    let controller = SettingsController.scripted(commands: commands)
    let reading = Task { await controller.readPictures() }
    try await settle("the reading never started") { commands.ran.count == 1 }
    await controller.choosePictures(PicturesSettings(keepDays: 21))
    gate.open()
    await reading.value
    #expect(controller.pictures.confirmed == PicturesSettings(keepDays: 21))
    #expect(controller.state.general.keepDays == 21)
    #expect(controller.state.general.canDeletePictures, "its count is kept all the same")
  }

  @Test("Delete All… asks first, deletes once confirmed, and reads the count again")
  func deleteAllAsksFirst() async throws {
    let commands = ScriptedCommands([
      "config --pictures-summary": Self.summary,
      "config --delete-pictures": Self.written,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readPictures()
    controller.send(.askDeletePictures)
    #expect(commands.ran == [Action.picturesSummary.arguments], "nothing is deleted until the person confirms")
    let alert = try #require(controller.state.alert)
    #expect(alert.title == SettingsWords.deletePicturesTitle)
    #expect(alert.message.contains("1,240 pictures") || alert.message.contains("\(1240.formatted()) pictures"))
    commands.answer("config --pictures-summary", #"{"ok": true, "config": {}, "pictures": {"count": 0, "bytes": 0}}"#)
    await controller.confirmAlert()
    #expect(
      commands.ran == [
        Action.picturesSummary.arguments, Action.deletePictures.arguments, Action.picturesSummary.arguments,
      ]
    )
    #expect(controller.state.alert == nil)
    #expect(controller.state.general.storedPictures == "No pictures")
    #expect(!controller.state.general.canDeletePictures)
  }
}
