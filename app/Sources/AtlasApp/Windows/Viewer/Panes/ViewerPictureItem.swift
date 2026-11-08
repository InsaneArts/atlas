/// One picture the sheet can show: a screenshot by its file name, or the screen just before a step, by the row that
/// kept it.
enum ViewerPictureItem: Equatable {
  /// A screenshot.
  ///
  /// - Parameter name: Its file name.
  case shot(String)
  /// The screen just before a step.
  ///
  /// - Parameter row: The row that kept the picture.
  case before(String)
}
