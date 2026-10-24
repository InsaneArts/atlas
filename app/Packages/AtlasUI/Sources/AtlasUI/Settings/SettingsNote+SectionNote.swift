extension SettingsNote {
  /// The note as a line under its section: small print, a spinner and words, or the orange problem line.
  var sectionNote: GroupedSectionNote {
    switch kind {
    case .quiet: .text(text)
    case .working: .status(.working, text)
    case .problem: .status(.problem, text)
    }
  }
}
