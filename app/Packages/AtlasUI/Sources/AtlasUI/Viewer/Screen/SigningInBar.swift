import SwiftUI

/// The bar over the viewer's Screen while the person signs in with agents paused: a lock, what to do and that agents
/// can't see, and Cancel and Done, Done in the accent; the question's banner's geometry and family, opaque and dark
/// in either appearance. Where one row doesn't fit, the buttons go under the words.
struct SigningInBar: View {
  /// The bar.
  let state: SigningInBarState
  /// Receives Done and Cancel.
  let send: @MainActor (ViewerEvent) -> Void

  /// The bar, in one row where it fits.
  var body: some View {
    ViewThatFits(in: .horizontal) {
      HStack(spacing: AskBannerLayout.gap) {
        words
          .frame(minWidth: AskBannerLayout.leadInRow, idealWidth: AskBannerLayout.leadInRow, maxWidth: .infinity)
        buttons
      }
      VStack(alignment: .leading, spacing: AskBannerLayout.gap) {
        words
        buttons
          .frame(maxWidth: .infinity, alignment: .trailing)
      }
    }
    .padding(AskBannerLayout.inset)
    .frame(maxWidth: AskBannerLayout.width)
    .background(shape.fill(Color.fillOverPicture))
    .overlay(shape.strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline))
    .background { ChipShadow(shape: shape) }
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .contain)
  }

  /// The lock and the words.
  private var words: some View {
    HStack(spacing: AskBannerLayout.gap) {
      Image(systemName: SymbolName.lockFilled.rawValue)
        .font(Typography.font(size: IconSize.s))
        .foregroundStyle(Color.textOverPicture)
        .frame(width: AskCardLayout.whoIcon)
        .accessibilityHidden(true)
      Text(state.text)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textOverPicture)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
  }

  /// Cancel, then Done.
  private var buttons: some View {
    HStack(spacing: AskBannerLayout.gap) {
      Button("Cancel") { send(.signInCancel) }
        .buttonStyle(.capsule(.secondary, size: .compact))
      Button("Done") { send(.signInDone) }
        .buttonStyle(.capsule(.accent, size: .compact, isWorking: state.isWorking))
    }
    .disabled(state.isWorking)
    .fixedSize()
  }

  /// The bar's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: AskBannerLayout.corner, style: .continuous)
  }
}
