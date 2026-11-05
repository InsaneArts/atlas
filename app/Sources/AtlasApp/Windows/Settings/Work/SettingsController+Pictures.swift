import AtlasCommands
import AtlasUI

/// General's pictures of agent actions: read with how many this Mac holds through `atlas config --pictures-summary`,
/// written there as the person changes them, one write at a time, and deleted once the person confirms.
extension SettingsController {
  /// Reads how pictures are kept and how many this Mac holds. A reading that does not answer, began before the
  /// person's newest choice, or ends while a write is under way leaves the choices as they are; its count is kept.
  func readPictures() async {
    let choices = pictures.choices
    let outcome = await dependencies.commands.run(.picturesSummary)
    guard let summary = PicturesSummary.of(outcome) else { return }
    pictures.stored = (summary.count, summary.bytes)
    guard choices == pictures.choices, !pictures.isWriting else { return }
    pictures.shown = summary.settings
    pictures.confirmed = summary.settings
  }

  /// Writes the person's choices to Atlas's settings. The pane shows them at once. A write under way writes the newest
  /// choices once it ends, so writes never overlap; one that fails puts the pane back to what the settings last
  /// confirmed and says why under its pictures.
  ///
  /// - Parameter wanted: How pictures are to be kept.
  func choosePictures(_ wanted: PicturesSettings) async {
    pictures.choices += 1
    pictures.shown = wanted
    guard !pictures.isWriting else { return }
    pictures.isWriting = true
    defer { pictures.isWriting = false }
    while let next = SettingsPictures.nextWrite(from: pictures.confirmed, to: pictures.shown) {
      let outcome = await dependencies.commands.run(next.action)
      guard outcome.ok else {
        pictures.shown = pictures.confirmed
        return say(Words.sentence(outcome.message), in: .general, kind: .problem)
      }
      pictures.confirmed = next.written
    }
    if session.notice?.tab == .general { say(nil, in: .general) }
  }

  /// Applies the General pane's pictures as the person left them: the switch, the days or Custom…, and the typed
  /// text's limit, Limited… keeping 80 characters until the person changes it.
  ///
  /// - Parameter general: The pane as the person left it.
  func updatePictures(_ general: SettingsGeneralState) {
    var wanted = pictures.shown
    wanted.isOn = general.picturesBeforeActions
    pictures.customDays = general.keepFor == .custom
    wanted.keepDays =
      general.keepFor.days
      ?? min(
        max(general.keepDays, SettingsGeneralState.keepDaysRange.lowerBound),
        SettingsGeneralState.keepDaysRange.upperBound
      )
    switch general.typedText {
    case .full: wanted.typedTextLimit = nil
    case .limited:
      wanted.typedTextLimit = min(
        max(general.typedTextLimit, SettingsGeneralState.typedTextRange.lowerBound),
        SettingsGeneralState.typedTextRange.upperBound
      )
    }
    guard wanted != pictures.shown else { return }
    // Unstructured, as an edit is synchronous: the change runs beside it.
    Task { await choosePictures(wanted) }
  }

  /// Deletes every picture of agent actions once the person confirmed. One that fails says why under the pictures;
  /// the count is read again either way.
  func deletePictures() async {
    let outcome = await dependencies.commands.run(.deletePictures)
    session.alert = nil
    say(outcome.ok ? nil : Words.sentence(outcome.message), in: .general, kind: .problem)
    await readPictures()
  }
}
