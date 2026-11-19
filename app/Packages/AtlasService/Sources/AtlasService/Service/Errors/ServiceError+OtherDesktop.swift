extension ServiceError {
  /// A reply for the desktop viewed before the app viewed another: what it says is that desktop's, so it is dropped.
  public static let otherDesktop = ServiceError("Atlas now shows another desktop.", code: "other-desktop")

  /// Whether the reply was dropped for being of the desktop viewed before.
  public var isOtherDesktop: Bool { code == Self.otherDesktop.code }
}
