extension StatusLine.Kind {
  /// How a checklist line stands, as a status line's kind: running is work, failed a problem, passed done.
  ///
  /// - Parameter state: How the line stands.
  init(_ state: CheckState) {
    switch state {
    case .busy: self = .working
    case .failed: self = .problem
    case .done: self = .done
    }
  }
}
