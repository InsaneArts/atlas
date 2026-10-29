extension ViewerActivityKind {
  /// The glyph a step of this kind shows in its disc.
  public var symbol: SymbolName {
    switch self {
    case .pointer: .pointer
    case .typing: .typing
    case .keys: .keyboard
    case .scroll: .scroll
    case .screenshot: .camera
    case .window: .window
    case .command: .terminal
    case .person: .person
    case .control: .handRaised
    case .problem: .exclamation
    }
  }
}
