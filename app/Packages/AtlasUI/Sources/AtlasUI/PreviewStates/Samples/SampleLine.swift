#if DEBUG
  /// One line of the line swap's samples: an agent's action, known by its step, and how long ago it was.
  struct SampleLine: Equatable, Sendable {
    /// The actions the samples step through, in turn.
    private static let actions = [
      "Clicked Ring in Opie",
      "Scrolled in Opie",
      "Turning Show Center off in Opie",
      "Typed \u{201C}weekly notes\u{201D} in Notes",
    ]

    /// Which line it is.
    let id: Int
    /// What the agent did.
    let words: String
    /// How long ago, which changes in place.
    let age: String

    /// The line at `step`, cycling through the actions.
    ///
    /// - Parameter step: How many lines came before it.
    /// - Returns: The line.
    static func line(at step: Int) -> SampleLine {
      SampleLine(id: step, words: actions[step % actions.count], age: "just now")
    }
  }
#endif
