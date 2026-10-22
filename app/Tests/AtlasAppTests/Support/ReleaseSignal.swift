/// A gate a test opens by hand: a scripted command waits at it, so the test acts while the command is under way.
@MainActor
final class ReleaseSignal {
  /// Whether the gate is open.
  private var isOpen = false
  /// Those waiting at the gate.
  private var waiting: [CheckedContinuation<Void, Never>] = []

  /// Waits until the gate opens; at once when it is open.
  func wait() async {
    guard !isOpen else { return }
    await withCheckedContinuation { waiting.append($0) }
  }

  /// Opens the gate, letting everyone waiting go on.
  func open() {
    isOpen = true
    for continuation in waiting {
      continuation.resume()
    }
    waiting = []
  }
}
