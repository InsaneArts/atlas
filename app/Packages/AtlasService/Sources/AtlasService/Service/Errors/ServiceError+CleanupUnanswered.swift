extension ServiceError {
  /// Whether a cleanup's apply got no reply, as when it ran past the client's wait or the request failed on the way:
  /// the service may still be cleaning up, so the app waits until `cleanup.json` shows no step under way rather than
  /// offering Clean Up again.
  public var isCleanupUnanswered: Bool { code == ViewerClient.unansweredCleanupCode }
}
