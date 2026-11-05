import AtlasCommands

/// General's pictures of agent actions as Settings holds them: what the pane shows, what Atlas's settings last
/// confirmed, whether a write is under way, how many times the person chose, so a reading begun before a choice is
/// never kept over it, whether they chose a custom number of days, and how many pictures this Mac holds.
struct SettingsPictures {
  /// What the pane shows: the person's newest choices, or what the settings say. Atlas's defaults until read.
  var shown = PicturesSettings()
  /// What Atlas's settings last said or were last written to.
  var confirmed = PicturesSettings()
  /// Whether a write is under way; one at a time.
  var isWriting = false
  /// How many times the person chose.
  var choices = 0
  /// Whether the person chose Custom…, so the days show as a number even at three or seven.
  var customDays = false
  /// How many pictures this Mac holds and their size, in bytes; nil until read.
  var stored: (count: Int, bytes: Int)?

  /// The next write that brings the settings to what the pane shows, and the settings once it is done: the switch
  /// first, then the days, then the typed text's limit.
  ///
  /// - Parameters:
  ///   - confirmed: What the settings last confirmed.
  ///   - wanted: What the pane shows.
  /// - Returns: The write and the settings after it, or nil when nothing differs.
  static func nextWrite(from confirmed: PicturesSettings, to wanted: PicturesSettings)
    -> (action: Action, written: PicturesSettings)?
  {
    var written = confirmed
    if confirmed.isOn != wanted.isOn {
      written.isOn = wanted.isOn
      return (.keepPictures(wanted.isOn), written)
    }
    if confirmed.keepDays != wanted.keepDays {
      written.keepDays = wanted.keepDays
      return (.keepPicturesFor(days: wanted.keepDays), written)
    }
    if confirmed.typedTextLimit != wanted.typedTextLimit {
      written.typedTextLimit = wanted.typedTextLimit
      return (.limitTypedText(wanted.typedTextLimit ?? 0), written)
    }
    return nil
  }
}
