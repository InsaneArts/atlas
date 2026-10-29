extension ActivityMoment {
  /// What a moment is titled by.
  public enum Subject: Equatable, Sendable {
    /// The app its actions were in.
    ///
    /// - Parameter app: The app.
    case app(ActivityApp)
    /// Commands, and anything else done in no app.
    case commands
    /// Screenshots alone.
    case screenshots

    /// What a moment in no app is titled by: Screenshots while every line took one, else Commands.
    ///
    /// - Parameter runs: The moment's lines.
    /// - Returns: The subject.
    static func appless(_ runs: [ActivityRun]) -> Subject {
      let shots = runs.allSatisfy { $0.entry.operation == ActivityGrouping.screenshotOperation }
      return shots ? .screenshots : .commands
    }
  }
}
