extension VMFacts {
  /// Where the four steps of making the VM stand: install macOS, start the VM, set up Atlas inside, connect to Atlas.
  /// The first not done is the current one.
  public var makingSteps: [StepProgress] {
    Self.inOrder([
      steps.contains(.created),
      steps.contains(.started),
      steps.contains(.installed),
      steps.contains(.closed),
    ])
  }

  /// Where the two permissions stand in the VM's window: Screen & System Audio Recording, then Accessibility. The first
  /// not allowed is the current one.
  public var allowSteps: [StepProgress] {
    let both = steps.contains(.allowed) || setup == .resuming || setup == .done
    return Self.inOrder([both || recordingAllowed, both || accessibilityAllowed])
  }

  /// Each of `done` done, and the first that is not current, the rest waiting.
  ///
  /// - Parameter done: Whether each step is done, in order.
  /// - Returns: Where each stands.
  static func inOrder(_ done: [Bool]) -> [StepProgress] {
    let current = done.firstIndex(of: false)
    return done.indices.map { index in
      if done[index] { return .done }
      return index == current ? .current : .waiting
    }
  }
}
