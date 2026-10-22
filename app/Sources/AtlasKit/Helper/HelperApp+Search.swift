extension HelperApp {
  /// The app as a search for apps keeps it: whole when its name holds the search, else with only the windows whose
  /// titles hold it, and gone when nothing does.
  ///
  /// - Parameter query: The search; empty, or spaces alone, keeps every app whole.
  /// - Returns: The app, with the windows the search keeps, or nil when it keeps nothing of it.
  package func matching(_ query: String) -> HelperApp? {
    let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !words.isEmpty, !name.localizedStandardContains(words) else { return self }
    var kept = self
    kept.windows = windows.filter { ($0.title ?? "").localizedStandardContains(words) }
    return kept.windows.isEmpty ? nil : kept
  }
}
