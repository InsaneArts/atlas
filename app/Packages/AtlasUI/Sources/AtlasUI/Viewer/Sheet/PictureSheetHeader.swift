import SwiftUI

/// The sheet's title row: the app's icon at 24, what happened in the heading style, which may hold a keycap, then the
/// app, the time and the session on one quiet line; Before and After at the end when there is a picture after.
struct PictureSheetHeader: View {
  /// The sheet.
  let sheet: ViewerPictureSheet
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The row.
  var body: some View {
    HStack(spacing: ViewerLayout.outlineLeadGap) {
      ActivityGlyph(icon: sheet.icon, side: IconSize.m)
      VStack(alignment: .leading, spacing: .zero) {
        ActivityStepWords(words: sheet.title, style: .heading, weight: .semibold)
          .accessibilityAddTraits(.isHeader)
        Text(verbatim: sheet.line)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
          .truncationMode(.tail)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      if sheet.hasAfter {
        Tabs(state: sides, style: .segment) { send(.pictureSide($0)) }
          .controlSize(.small)
          .fixedSize()
      }
    }
    .padding(.top, ViewerLayout.sheetHeaderInset)
    .padding(.horizontal, ViewerLayout.sheetHeaderInset)
    .padding(.bottom, ViewerLayout.sheetInset)
  }

  /// Before and After, the side shown chosen.
  private var sides: CapsuleTabsState<ViewerPictureSide> {
    CapsuleTabsState(
      tabs: [
        CapsuleTab(id: .before, title: "Before", help: "The screen just before this step"),
        CapsuleTab(id: .after, title: "After", help: "The screen just after this step"),
      ],
      selected: sheet.shownSide,
      label: "Picture"
    )
  }
}
