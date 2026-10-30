#if DEBUG
  import CoreGraphics
  import Foundation

  /// A debug renderer's geometry observation, independent of accessibility or Window Server availability.
  public struct LayoutFrameReport: Equatable {
    private let identity = UUID()
    /// Receives a named element's presentation geometry as SwiftUI lays it out.
    let report: @MainActor (String, CGRect) -> Void

    /// Creates a geometry observation.
    ///
    /// - Parameter report: Receives an element's identity and frame.
    public init(report: @escaping @MainActor (String, CGRect) -> Void) {
      self.report = report
    }

    /// Whether both values name the same observer.
    ///
    /// - Parameters:
    ///   - lhs: One observer.
    ///   - rhs: Another observer.
    /// - Returns: Whether their identities match.
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.identity == rhs.identity }
  }
#endif
