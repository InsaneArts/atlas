extension HelperMode {
  /// Whether the helper is under this viewer's own control: the viewer holds a lease, and the helper answers with
  /// nothing to recover first. The mode and the reason a viewer shows both follow it.
  ///
  /// - Parameter holdingControl: Whether this viewer holds control.
  /// - Returns: Whether this viewer's control is what the helper is doing.
  package func isControlled(byThisViewer holdingControl: Bool) -> Bool {
    holdingControl && self != .recovering && answers
  }
}
