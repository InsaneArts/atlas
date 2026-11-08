import AtlasUI

extension [ViewerPictureTile] {
  /// Marks the picture the sheet was opened from to take focus back as the sheet closes.
  ///
  /// - Parameter opener: The picture opened.
  mutating func mark(_ opener: ViewerPictureItem) {
    guard case .shot(let name) = opener else { return }
    for index in indices where self[index].id == name { self[index].returnsFocus = true }
  }
}
