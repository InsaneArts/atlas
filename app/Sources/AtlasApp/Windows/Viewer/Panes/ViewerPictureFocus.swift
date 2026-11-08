import AtlasUI

/// The picture the sheet shows: which run's pictures it steps through, which of them, whether it steps through the
/// run's screenshots or every step's picture, and which of a step's pictures shows.
struct ViewerPictureFocus: Equatable {
  /// The run whose pictures ‹ and › step through.
  var run: String
  /// The picture shown.
  var item: ViewerPictureItem
  /// Whether ‹ and › step through every step's picture, as a step's picture button opens it, rather than the run's
  /// screenshots alone.
  var everyStep: Bool
  /// Which of a step's pictures shows.
  var side = ViewerPictureSide.before
  /// What the sheet was opened from, which takes focus back as it closes while the picture shown is not in the
  /// outline: a screenshot by its name, or a step by its line's identity; nil when not known.
  var opener: ViewerPictureItem?
  /// What the footer briefly says after Copy or Show in Finder, until it is said long enough or another picture shows.
  var notice: ViewerSheetNotice?
}
