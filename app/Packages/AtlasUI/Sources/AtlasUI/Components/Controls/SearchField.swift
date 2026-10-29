import SwiftUI

/// A pane's search field: a magnifying glass, then the query, and a clear button while there is one, on a quiet
/// capsule with no edge, 200 wide where there is room and narrower, down to 120, where there is not. Focused, it draws a fine
/// accent ring and its soft halo, as a form's fields do; in a bar it fills, it takes the bar's width instead. A click
/// anywhere on the capsule takes the focus; a click anywhere else in the window gives it up, as its window learns where
/// it stands while it holds the keyboard (``EnvironmentValues/focusedFieldReport``). Escape empties the query, and a
/// second Escape gives up the focus. Giving it up with no query, by Escape or by a click elsewhere, tells the owner,
/// which may take it as closing the field; with a query the field keeps it. It holds its own focus, and takes it each
/// time its owner asks (`focusRequests`), as Find does; the query belongs to the owner, which receives each edit.
struct SearchField: View {
  /// The query as the owner holds it.
  let text: String
  /// What the empty field says, such as "Search activity".
  let prompt: String
  /// Counts the owner's asks for the focus: each change takes it.
  var focusRequests = 0
  /// Whether it takes the focus as it appears, as in a panel the person has just opened to search; no by default.
  var focusesOnAppear = false
  /// Whether it takes all the width offered, as in a bar it fills, rather than its own 120 to 200.
  var fills = false
  /// Runs as the field gives up the focus with no query, on Escape or a click elsewhere, for an owner that then closes
  /// it; nothing by default.
  var onExit: (@MainActor () -> Void)?
  /// Receives each edit.
  let onChange: @MainActor (String) -> Void
  @FocusState private var focused: Bool
  /// Where the capsule stands in the window, for the window to tell a click on it from one outside it.
  @State private var frame = CGRect.null
  @Environment(\.isEnabled) private var isEnabled
  @Environment(\.focusedFieldReport) private var report

  /// The magnifier, the query and its clear button on their quiet capsule.
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
      if !text.isEmpty {
        Button {
          onChange("")
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
    .frame(
      minWidth: fills ? nil : SearchFieldLayout.leastWidth,
      idealWidth: fills ? nil : SearchFieldLayout.width,
      maxWidth: fills ? .infinity : SearchFieldLayout.width
    )
    .frame(height: ControlHeight.s)
    .background(
      Capsule()
        .fill(Color.fillControl)
    )
    .overlay { focusRing }
    .animation(Motion.fade, value: focused)
    .contentShape(Capsule())
    .onTapGesture { focused = true }
    .onGeometryChange(for: CGRect.self) { proxy in
      proxy.frame(in: .global)
    } action: { capsule in
      frame = capsule
      if focused { report?.report(capsule) }
    }
    .onChange(of: focused) { _, isFocused in
      report?.report(isFocused ? frame : nil)
      if !isFocused, text.isEmpty { onExit?() }
    }
    .onChange(of: isEnabled) { _, enabled in
      if !enabled { focused = false }
    }
    .onChange(of: focusRequests) { if isEnabled { focused = true } }
    .onAppear { if focusesOnAppear && isEnabled { focused = true } }
    .onDisappear {
      if focused {
        focused = false
        report?.report(nil)
      }
    }
  }

  /// The fine accent edge and its soft halo along the capsule, showing while the field has the focus.
  private var focusRing: some View {
    CapsuleEdge()
      .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
      .background(
        CapsuleEdge()
          .stroke(Color.accentRing, lineWidth: SearchFieldLayout.haloWidth)
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

  /// Escape: empties the query, or with none, gives up the focus, which tells the owner.
  ///
  /// - Returns: Handled, so the key goes no further.
  private func escape() -> KeyPress.Result {
    if text.isEmpty {
      focused = false
    } else {
      onChange("")
    }
    return .handled
  }
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
