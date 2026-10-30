import SwiftUI

@testable import AtlasUI

/// Reads the same environment that gates Activity arrivals, inside the shared scroll's content boundary.
struct ScrollTopProbe: View {
  /// Receives the scroll's actual top classification.
  let report: (Bool) -> Void
  @Environment(\.listIsAtTop) private var isAtTop

  /// Enough content to scroll, reporting each classification change.
  var body: some View {
    Color.clear
      .frame(height: 2000)
      .onChange(of: isAtTop, initial: true) { _, value in report(value) }
  }
}
