/// The titles of the menu items chosen, in order.
final class ChoiceCount {
  /// The titles, in the order chosen.
  private(set) var titles: [String] = []

  /// Counts one choice.
  ///
  /// - Parameter title: The item's title.
  func add(_ title: String) {
    titles.append(title)
  }
}
