#if DEBUG
  /// Where the sample chat app stands in a task: on another channel, on #general with an empty message box, with the
  /// release note typed, or with it posted.
  public enum SampleChatStage: String, CaseIterable, Sendable {
    /// On #announcements, before the agent picked #general.
    case start
    /// On #general, the message box empty.
    case empty
    /// On #general, the release note typed and not yet sent.
    case typed
    /// On #general, the release note posted.
    case posted
  }
#endif
