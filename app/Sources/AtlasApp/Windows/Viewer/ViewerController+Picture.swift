import AtlasUI

/// The picture on screen: copied or saved as it is, never captured anew; drawn at actual size; and how often it is
/// read.
extension ViewerController {
  /// Copy Picture: the frame on screen goes to the clipboard.
  func copyPicture() {
    guard let picture = reading.picture else { return }
    pictures.copy(picture.image)
    say(ViewerWords.pictureCopied)
  }

  /// Save Picture…: asks where, then writes the frame on screen as a PNG. Nothing is asked of the helper.
  func savePicture() {
    guard let picture = reading.picture else { return }
    if pictures.save(picture.image) == false { say(ViewerWords.pictureNotSaved) }
  }

  /// Actual Size: draws the screen at one view point a display point, in a view that scrolls, or fits it again.
  func toggleActualSize() {
    guard reading.picture != nil || session.actualSize else { return }
    session.actualSize.toggle()
  }

  /// One of the ⋯ menu's picture choices: Live, every five seconds, or paused.
  ///
  /// - Parameter choice: The choice.
  func choosePicture(_ choice: ViewerPictureChoice) {
    switch choice {
    case .live, .everyFiveSeconds:
      setRate(choice == .live ? .live : .everyFiveSeconds)
      if session.paused { togglePause() }
    case .paused:
      if !session.paused { togglePause() }
    }
  }
}
