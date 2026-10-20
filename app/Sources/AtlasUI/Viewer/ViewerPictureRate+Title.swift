extension ViewerPictureRate {
  /// The rate as the refresh picker names it.
  var title: String {
    switch self {
    case .everySecond: "Every second"
    case .everyTwoSeconds: "Every 2 seconds"
    case .everyFiveSeconds: "Every 5 seconds"
    }
  }
}
