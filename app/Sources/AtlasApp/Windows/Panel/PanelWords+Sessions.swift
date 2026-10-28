/// The panel's words for host sessions that no other window shows.
extension PanelWords {
  /// The row under Sessions that opens every session, while there are more than the rows show.
  ///
  /// - Parameter count: How many more sessions there are.
  /// - Returns: "+2 more".
  static func moreSessions(_ count: Int) -> String {
    "+\(count) more"
  }

  /// What VoiceOver reads for a session's row: the name, the state, then the line when there is one.
  ///
  /// - Parameters:
  ///   - name: The session's name.
  ///   - state: Its state in a word.
  ///   - line: What it does or did last; empty for none.
  /// - Returns: "Claude Code · understudy, Acting, Clicking Show Center in Opie".
  static func sessionLabel(name: String, state: String, line: String) -> String {
    [name, state, line]
      .filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
