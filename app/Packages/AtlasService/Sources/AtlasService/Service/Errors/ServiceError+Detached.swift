extension ServiceError {
  /// The app is not attached to the Atlas service: it is starting, restarting or not running.
  public static let detached = ServiceError("Atlas is reconnecting to its service.", code: "detached")
}
