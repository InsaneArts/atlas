import SwiftUI

/// The title, which swaps by opacity as it changes to the done title.
struct AlertCardTitle: View {
  /// The title shown.
  let text: String

  /// The component drawn from its current state.
  var body: some View {
    Text(text)
      .textStyle(.body, weight: .bold)
      .multilineTextAlignment(.center)
      .fixedSize(horizontal: false, vertical: true)
      .swapsText(on: text)
      .accessibilityAddTraits(.isHeader)
  }
}
