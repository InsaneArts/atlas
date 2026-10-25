extension ServiceError {
  /// A picture whose size cannot be read: the service named no display size, or its PNG would not decode. Input is
  /// mapped through both, so neither picture is shown.
  public static let unknownScreenSize = ServiceError("The helper's screen size is unknown.")
}
