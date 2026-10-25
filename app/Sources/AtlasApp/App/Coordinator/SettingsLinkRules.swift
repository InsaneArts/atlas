/// Settings' rules for the main window's picture, which Settings shows and changes from General: the open main
/// window's own choice, or, while none is open, the rate saved for the next one.
enum SettingsLinkRules {
  /// The picture Settings shows: the open main window's choice, else the saved rate's (live when none is saved).
  ///
  /// - Parameters:
  ///   - viewer: The open main window's controller, or nil when none is open.
  ///   - savedRate: The rate saved for the next main window, in seconds; 0 when none is saved.
  /// - Returns: The choice.
  static func picture(viewer: ViewerController?, savedRate: Double) -> ViewerPictureChoice {
    if let viewer {
      if viewer.session.paused { return .paused }
      return viewer.readingLoop.rate == .everyFiveSeconds ? .everyFiveSeconds : .live
    }
    let rate = ViewerPictureRate(rawValue: savedRate) ?? .live
    return rate == .everyFiveSeconds ? .everyFiveSeconds : .live
  }

  /// A choice made in Settings: the open main window takes it, or, while none is open, its rate is saved for the next
  /// one. Paused is a window's own, so with none open it saves nothing.
  ///
  /// - Parameters:
  ///   - choice: The choice.
  ///   - viewer: The open main window's controller, or nil when none is open.
  ///   - preferences: Where the next main window's rate is saved.
  static func choose(_ choice: ViewerPictureChoice, viewer: ViewerController?, preferences: ViewerPreferences) {
    if let viewer {
      viewer.choosePicture(choice)
    } else if choice != .paused {
      preferences.saveRate((choice == .everyFiveSeconds ? ViewerPictureRate.everyFiveSeconds : .live).seconds)
    }
  }
}
