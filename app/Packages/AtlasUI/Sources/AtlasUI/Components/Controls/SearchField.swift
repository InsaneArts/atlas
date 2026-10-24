import SwiftUI

/// A pane's search field: a magnifying glass, then the query, on a quiet rounded field, 200 wide where there is room
/// and narrower, down to 120, where there is not. It holds its own focus; the query belongs to the owner, which
/// receives each edit.
struct SearchField: View {
  /// The query as the owner holds it.
  let text: String
  /// What the empty field says, such as "Search activity".
  let prompt: String
  /// Receives each edit.
  let onChange: @MainActor (String) -> Void
  @FocusState private var focused: Bool

  /// The glass and the query on their quiet rounded field.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      Image(systemName: SymbolName.search.rawValue)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
        .accessibilityHidden(true)
      TextField(text: query, prompt: Text(prompt).foregroundStyle(Color.textTertiary)) {
        Text(prompt)
      }
      .textFieldStyle(.plain)
      .font(Typography.font(.callout))
      .focused($focused)
    }
    .padding(.horizontal, Spacing.sPlus)
    .frame(minWidth: Layout.leastWidth, idealWidth: Layout.width, maxWidth: Layout.width)
    .frame(height: ControlHeight.s)
    .background(
      RoundedRectangle(cornerRadius: Radius.small)
        .fill(Color.fillControl)
    )
    .overlay(
      RoundedRectangle(cornerRadius: Radius.small)
        .strokeBorder(focused ? Color.strokeControl : Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .contentShape(RoundedRectangle(cornerRadius: Radius.small))
    .onTapGesture { focused = true }
  }

  /// The query, with each edit sent to the owner, which holds it.
  private var query: Binding<String> {
    Binding(
      get: { text },
      set: { onChange($0) }
    )
  }
}

/// The field's own measures, from v3's pane header (`.search`).
private enum Layout {
  /// 200: the field's width where there is room.
  static let width: CGFloat = 200
  /// 120: the least it narrows to, still holding a short query such as a time beside its glass.
  static let leastWidth: CGFloat = 120
}

#if DEBUG
  #Preview("Samples") {
    SearchFieldSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var text = ""
    InteractivePreview {
      Text(verbatim: "Query: \(text)")
    } content: {
      SearchField(text: text, prompt: "Search activity") { edit in
        text = edit
      }
    }
  }
#endif
