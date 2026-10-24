extension ViewerActivityKind {
  /// The symbol a line of this kind shows in its tile (v3 V8's nine).
  public var symbol: SymbolName {
    switch self {
    case .pointer: .click
    case .keys: .keyboard
    case .scroll: .scroll
    case .screenshot: .camera
    case .window: .window
    case .command: .terminal
    case .person: .person
    case .control: .handRaised
    case .problem: .problem
    }
  }
}
