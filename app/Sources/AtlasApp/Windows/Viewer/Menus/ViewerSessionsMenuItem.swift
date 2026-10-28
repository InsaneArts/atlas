import AtlasUI

/// One host session in the sessions menu: its key, which the item represents, its name, its state and line, and its
/// host's icon.
struct ViewerSessionsMenuItem: Equatable {
  /// The session's key; empty for Terminal.
  var key: String
  /// The session's name.
  var title: String
  /// Its state and line, such as "Acting · has the helper".
  var subtitle: String
  /// The host's icon.
  var icon: RowIcon
}
