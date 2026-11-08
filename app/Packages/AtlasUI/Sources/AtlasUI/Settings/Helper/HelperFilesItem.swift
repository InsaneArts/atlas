/// One row of the Helper pane's "Your files" section.
enum HelperFilesItem: Identifiable, Equatable {
  /// Whether the home folder is private from the helper.
  case home(SettingsFilesRow)
  /// Whether agents may copy sign-ins into the helper.
  case signIns(Bool)

  /// The row's place, which stays as its words change.
  var id: Int {
    switch self {
    case .home: 0
    case .signIns: 1
    }
  }
}
