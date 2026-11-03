extension ViewerService {
  /// No frame service, unless a service offers one.
  ///
  /// - Returns: Nil.
  public func framePipe() async throws -> FramePipeDescriptor? {
    nil
  }
}
