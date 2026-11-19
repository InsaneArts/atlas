extension VMFacts {
  /// Where the setup of a VM's add stands once its window opened.
  enum Setup: String, Decodable, Sendable {
    /// Its window is open, and the guest waits for the two permissions.
    case waiting
    /// Both are allowed: its window's input is being closed, and agents let go on.
    case resuming
    /// Set up: agents wait for the person's Resume.
    case done
  }
}
