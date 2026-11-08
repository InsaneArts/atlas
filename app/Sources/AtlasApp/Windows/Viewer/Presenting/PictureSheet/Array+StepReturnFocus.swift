import AtlasUI

extension [ViewerActivityStep] {
  /// Marks the step whose picture opened the sheet, or whose screenshot did, to take focus back as the sheet closes.
  ///
  /// - Parameter opener: What opened the sheet: a step by its line's identity, or a screenshot by its name.
  mutating func mark(_ opener: ViewerPictureItem) {
    for index in indices {
      switch opener {
      case .before(let row) where self[index].id == row && self[index].hasPicture:
        self[index].returnsFocus = true
      case .shot(let name) where self[index].inPlace?.id == name:
        self[index].inPlace?.returnsFocus = true
      default:
        continue
      }
    }
  }
}
