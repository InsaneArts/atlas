extension ServiceError {
  /// Whether the request was cancelled before its reply, as a reading stopped part way, rather than refused.
  public var isCancelled: Bool { code == ViewerClient.cancelledCode }
}
