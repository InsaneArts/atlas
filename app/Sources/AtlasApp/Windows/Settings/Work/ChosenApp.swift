/// An app the person chose to add to the helper's accounts: its bundle identifier and the name it shows.
struct ChosenApp: Equatable {
  /// The app's bundle identifier, such as `com.tinyspeck.slackmacgap`.
  var bundleID: String
  /// The name the app shows, such as "Slack".
  var name: String
}
