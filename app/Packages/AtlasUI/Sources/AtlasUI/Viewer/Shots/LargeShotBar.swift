import SwiftUI

/// The large screenshot's one glass bar: its time and size, Copy, Show in Finder and Close.
struct LargeShotBar: View {
  /// The screenshot shown large.
  let shot: ViewerLargeShot
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The time and size, then the three buttons, on glass.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      Text(shot.caption)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
        .padding(.trailing, Spacing.xs)
      Button {
        send(.copyShot)
      } label: {
        labelled(.copy, "Copy")
      }
      .buttonStyle(.capsule(.glass, size: .mini))
      .help("Copy this screenshot as taken")
      Button {
        send(.showShotInFinder)
      } label: {
        labelled(.folder, "Show in Finder")
      }
      .buttonStyle(.capsule(.glass, size: .mini))
      .disabled(!shot.canShowInFinder)
      .help("Show this screenshot's file in the Finder")
      Button(String("Close")) { send(.closeShot) }
        .buttonStyle(.capsule(.primary, size: .mini))
        .help("Back to the screenshots")
    }
    .padding([.vertical, .trailing], Layout.inset)
    .padding(.leading, Spacing.mPlus)
    .glass(.regular, in: Capsule())
    .elevation(.floating)
  }

  /// A button's symbol and word.
  ///
  /// - Parameters:
  ///   - symbol: The symbol.
  ///   - word: The word.
  /// - Returns: The label.
  private func labelled(_ symbol: SymbolName, _ word: String) -> some View {
    HStack(spacing: Spacing.xs) {
      Image(systemName: symbol.rawValue)
      Text(word)
    }
  }
}

/// The bar's own measures for its floating controls.
private enum Layout {
  /// 5: the bar's inset above, below and after its buttons; its words start 14 in.
  static let inset: CGFloat = 5
}

#if DEBUG
  #Preview("Samples") {
    if case .screenshots(let shots) = PreviewStates.viewerShotLarge.pane, let large = shots.large {
      LargeShotBar(shot: large) { _ in }
        .padding(Spacing.xxxl)
        .sampleSurface()
    }
  }
#endif
