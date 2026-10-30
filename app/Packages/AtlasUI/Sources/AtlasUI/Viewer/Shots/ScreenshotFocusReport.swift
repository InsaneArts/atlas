import Foundation

/// A stable report owned by the native window for its lifetime.
public struct ScreenshotFocusReport: Equatable, Sendable {
  private let identity = UUID()
  /// Receives whether screenshot content owns the focus.
  public let report: @MainActor @Sendable (Bool) -> Void

  /// Creates the window's focus report.
  ///
  /// - Parameter report: Receives focus changes from the hosted view.
  public init(report: @escaping @MainActor @Sendable (Bool) -> Void) {
    self.report = report
  }

  /// Whether both reports belong to the same window subscription.
  ///
  /// - Parameters:
  ///   - lhs: One subscription.
  ///   - rhs: Another subscription.
  /// - Returns: Whether their identities match.
  public static func == (lhs: Self, rhs: Self) -> Bool { lhs.identity == rhs.identity }
}
