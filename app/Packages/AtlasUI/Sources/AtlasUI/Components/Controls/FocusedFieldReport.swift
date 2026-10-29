import CoreGraphics

/// Where the field holding the keyboard stands, handed to the window that hosts it, so a click anywhere outside the
/// field can take the keyboard back while a click on any part of it, its magnifier and its margins included, keeps it.
/// Frames are in the window content's own space, from its top left; nil means no field holds the keyboard.
public struct FocusedFieldReport: Equatable, Sendable {
  /// Receives the focused field's frame, or nil once no field holds the keyboard.
  let report: @MainActor @Sendable (CGRect?) -> Void

  /// Creates the report.
  ///
  /// - Parameter report: Receives the focused field's frame, or nil once no field holds the keyboard.
  public init(report: @escaping @MainActor @Sendable (CGRect?) -> Void) {
    self.report = report
  }

  /// Whether two reports are the same: always, as the window sets one for its life.
  ///
  /// - Parameters:
  ///   - lhs: One report.
  ///   - rhs: The other.
  /// - Returns: True.
  public static func == (lhs: FocusedFieldReport, rhs: FocusedFieldReport) -> Bool { true }
}
