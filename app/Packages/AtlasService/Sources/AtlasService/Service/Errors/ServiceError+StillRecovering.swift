extension ServiceError {
  /// Whether a Recover gave no answer in time and may still be under way: the service carries on, so the app says
  /// it still runs and reads the status rather than offering Recover again.
  public var isStillRecovering: Bool { code == ViewerClient.stillRecoveringCode }
}
