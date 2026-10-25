extension StatusLine {
  /// A setup screen's work note as its status line: what runs now, with a spinner, or what went wrong, in
  /// orange; nothing when the note is empty.
  ///
  /// - Parameter note: The window's work note.
  init?(note: WorkNote) {
    switch note {
    case .none: return nil
    case .working(let text): self.init(kind: .working, text: text)
    case .problem(let text): self.init(kind: .problem, text: text)
    }
  }
}
