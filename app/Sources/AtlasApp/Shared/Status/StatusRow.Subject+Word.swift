import AtlasStatus

extension StatusRow.Subject {
  /// The check's name as the Ready card shows it.
  var word: String {
    switch self {
    case .check(let name): CheckWords.title(name)
    case .nativeDialogs: CheckWords.nativeDialogs
    }
  }
}
