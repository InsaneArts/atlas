import AppKit
import SwiftUI

/// A pane's search field: a magnifying glass, then the query, and a clear button while there is one, on a quiet
/// rounded field, 200 wide where there is room and narrower, down to 120, where there is not. Focused, it draws a fine
/// accent ring and its soft halo, as a form's fields do. Escape empties the query, and a second Escape gives up the
/// focus. It holds its own focus, and takes it each time its owner asks (`focusRequests`), as Find does; the query
/// belongs to the owner, which receives each edit.
struct SearchField: View {
  /// The query as the owner holds it.
  let text: String
  /// What the empty field says, such as "Search activity".
  let prompt: String
  /// Counts the owner's asks for the focus: each change takes it.
  var focusRequests = 0
  /// Receives each edit.
  let onChange: @MainActor (String) -> Void
  /// Optional owner-level result navigation, leaving ordinary text editing with the field.
  var onMove: (@MainActor (Int) -> Void)?
  /// Opens a selected location only after the text field commits Return.
  var onOpen: (@MainActor () -> Void)?
  @FocusState private var focused: Bool

  /// The glass, the query and its clear button on their quiet rounded field.
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
      .onKeyPress(.escape) { escape() }
      .onKeyPress(.upArrow) { move(-1) }
      .onKeyPress(.downArrow) { move(1) }
      .onSubmit { onOpen?() }
      if !text.isEmpty {
        Button {
          onChange("")
          focused = true
        } label: {
          Image(systemName: SymbolName.clearText.rawValue)
            .font(Typography.font(.callout))
            .foregroundStyle(Color.textTertiary)
        }
        .buttonStyle(.plain)
        .help(Text(verbatim: "Clear"))
        .accessibilityLabel(Text(verbatim: "Clear the search"))
      }
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
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .overlay { focusRing }
    .animation(Motion.fade, value: focused)
    .contentShape(RoundedRectangle(cornerRadius: Radius.small))
    .onTapGesture { focused = true }
    .onChange(of: focusRequests) { focused = true }
  }

  /// The fine accent edge and its soft halo just outside the field, showing while it has the focus.
  private var focusRing: some View {
    RoundedRectangle(cornerRadius: Radius.small)
      .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .stroke(Color.accentRing, lineWidth: Layout.haloWidth)
      )
      .opacity(focused ? 1 : 0)
      .allowsHitTesting(false)
  }

  /// The query, with each edit sent to the owner, which holds it.
  private var query: Binding<String> {
    Binding(
      get: { text },
      set: { onChange($0) }
    )
  }

  /// Escape: empties the query, or with none, gives up the focus.
  ///
  /// - Returns: Handled, so the key goes no further.
  private func escape() -> KeyPress.Result {
    guard !composing else { return .ignored }
    if text.isEmpty {
      focused = false
    } else {
      onChange("")
    }
    return .handled
  }

  /// Leaves input-method candidate navigation with the native field editor.
  private var composing: Bool {
    let editor = NSApp.keyWindow?.firstResponder as? NSTextView
    return editor?.hasMarkedText() == true
  }

  /// Routes result arrows only when the owner supports them and the query is nonempty.
  ///
  /// - Parameter step: The requested direction.
  /// - Returns: Whether result navigation consumed this key.
  private func move(_ step: Int) -> KeyPress.Result {
    guard !composing, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, let onMove else {
      return .ignored
    }
    onMove(step)
    return .handled
  }
}

/// The field's own measures, from the pane header's search.
private enum Layout {
  /// 200: the field's width where there is room.
  static let width: CGFloat = 200
  /// 120: the least it narrows to, still holding a short query beside its glass.
  static let leastWidth: CGFloat = 120
  /// 3: the focus ring's soft halo, as a form's field draws it.
  static let haloWidth: CGFloat = 3
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
