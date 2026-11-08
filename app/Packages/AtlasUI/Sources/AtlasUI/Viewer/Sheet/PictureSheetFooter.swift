import SwiftUI

/// The sheet's footer: ‹ and › through that run's pictures with where this one stands, such as "120 of 120 in this
/// TextEdit run"; then Copy, Show in Finder and Done, the default button, which Return presses. Escape closes too. On a
/// narrow window the position drops its run, and narrower still the buttons wrap to a second row.
struct PictureSheetFooter: View {
  /// The sheet.
  let sheet: ViewerPictureSheet
  /// The window's width.
  let window: CGFloat
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Whether Done has keyboard focus, as it takes on the sheet's arrival so the keyboard is in the sheet.
  @FocusState private var doneFocused: Bool

  /// The two sides of the footer, on one row or two.
  var body: some View {
    Group {
      if window <= ViewerLayout.outlineNarrowest {
        VStack(alignment: .trailing, spacing: ViewerLayout.sheetInset) {
          stepping
            .frame(maxWidth: .infinity, alignment: .leading)
          buttons
        }
      } else {
        HStack(spacing: ViewerLayout.sheetInset) {
          stepping
          Spacer(minLength: .zero)
          buttons
        }
      }
    }
    .padding(ViewerLayout.sheetInset)
    .background {
      // Escape closes the sheet as Done does.
      Button("Close") { send(.closePicture) }
        .keyboardShortcut(.cancelAction)
        .hidden()
        .accessibilityHidden(true)
    }
  }

  /// ‹ and ›, then where this picture stands.
  private var stepping: some View {
    HStack(spacing: Spacing.s) {
      HStack(spacing: ViewerLayout.sheetArrowGap) {
        Button {
          send(.previousPicture)
        } label: {
          Image(systemName: SymbolName.back.rawValue)
        }
        .buttonStyle(.capsule(.secondary, size: .small, outline: .circle))
        .disabled(!sheet.canGoBack)
        .keyboardShortcut(.leftArrow, modifiers: [])
        .help("The picture before")
        .accessibilityLabel(Text("Previous picture"))
        Button {
          send(.nextPicture)
        } label: {
          Image(systemName: SymbolName.next.rawValue)
        }
        .buttonStyle(.capsule(.secondary, size: .small, outline: .circle))
        .disabled(!sheet.canGoForward)
        .keyboardShortcut(.rightArrow, modifiers: [])
        .help("The picture after")
        .accessibilityLabel(Text("Next picture"))
      }
      Text(verbatim: position)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
        .lineLimit(1)
        .truncationMode(.tail)
    }
  }

  /// "120 of 120 in this TextEdit run", or on a narrow window "120 of 120".
  private var position: String {
    guard window > ViewerLayout.outlineNarrow, !sheet.run.isEmpty else { return sheet.position }
    return "\(sheet.position) \(sheet.run)"
  }

  /// Copy, Show in Finder and Done, in macOS order with the default button last.
  private var buttons: some View {
    HStack(spacing: Spacing.s) {
      Button("Copy") { send(.copyPicture) }
        .buttonStyle(.capsule(.secondary, size: .small))
        .help("Copy this picture")
      Button("Show in Finder") { send(.showPictureInFinder) }
        .buttonStyle(.capsule(.secondary, size: .small))
        .disabled(!sheet.canShowInFinder)
        .help("Show this picture's file in the Finder")
      Button("Done") { send(.closePicture) }
        .buttonStyle(.capsule(.accent, size: .small))
        .keyboardShortcut(.defaultAction)
        .focused($doneFocused)
        .onAppear { doneFocused = true }
    }
    .fixedSize()
  }
}
