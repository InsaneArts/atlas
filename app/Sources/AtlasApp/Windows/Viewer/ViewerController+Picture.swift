import AtlasUI

/// The picture on screen as the window shows it: drawn at actual size, and how often it is read. Copying and saving it
/// are the reading loop's (``ViewerReadingLoop``).
extension ViewerController {
  /// Actual Size: draws the screen at one view point a display point, in a view that scrolls, or fits it again.
  func toggleActualSize() {
    guard reading.picture != nil || session.actualSize else { return }
    session.actualSize.toggle()
  }

  /// One of the ⋯ menu's picture choices: Live, every five seconds, or paused.
  ///
  /// - Parameter choice: The choice.
  func choosePicture(_ choice: SettingsPicture) {
    switch choice {
    case .live, .everyFiveSeconds:
      readingLoop.setRate(choice == .live ? .live : .everyFiveSeconds)
      if session.paused { togglePause() }
    case .paused:
      if !session.paused { togglePause() }
    }
  }
}
