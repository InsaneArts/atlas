extension ViewerTab {
  /// Which way the stage moves going from one tab to another, by where the two sit in the bar: 1 to the right, so
  /// the new pane comes in from the right and the old one leaves to the left; −1 back to the left; 0 for the same tab
  /// (v3 X3).
  ///
  /// - Parameters:
  ///   - old: The tab left.
  ///   - new: The tab shown.
  /// - Returns: 1, −1 or 0.
  public static func direction(from old: ViewerTab, to new: ViewerTab) -> Int {
    let order = Array(allCases)
    let from = order.firstIndex(of: old) ?? .zero
    let to = order.firstIndex(of: new) ?? .zero
    let steps = to - from
    return steps.signum()
  }
}
