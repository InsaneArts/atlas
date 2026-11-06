import AtlasUI

/// The action shown large over the window, as the person left it: which row of which moment, how its pictures are
/// compared, and whether what stayed the same is dimmed.
struct ViewerActionFocus: Equatable {
  /// The moment it is in, whose actions Previous and Next step through.
  var moment: String
  /// Its row.
  var row: String
  /// How its pictures are compared.
  var compare = ViewerCompareMode.slide
  /// Whether what stayed the same is dimmed.
  var showsChanges = false
}
