extension InputAction {
  /// The action's type, as the service names it.
  var kind: String {
    switch self {
    case .pointer: "pointer"
    case .move: "move"
    case .scroll: "scroll"
    case .type: "type"
    case .key: "key"
    case .wait: "wait"
    }
  }

  /// A pointer or plain move, which a newer one may replace while queued.
  package var isMove: Bool {
    switch self {
    case .move: true
    case .pointer(let phase, _, _, _, _, _): phase == .move
    default: false
    }
  }
}
