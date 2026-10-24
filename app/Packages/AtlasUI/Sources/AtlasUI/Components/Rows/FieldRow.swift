import SwiftUI

/// A labelled text field, one row of a ``GroupedList`` (v3 D6): its name in a column on the left, the field filling the
/// rest. Focused, it draws a fine ring inside the row (v3 S6). Flagged, it shows what the line under the form is about
/// (v3 X12): the row washes orange, its name turns orange, it gains the round "!" at its end, and the focus moves to
/// it as the flag arrives, unless the person is typing in another field.
struct FieldRow: View {
  /// The field's name, on the left.
  let label: String
  /// What is typed.
  @Binding var text: String
  /// Whether the text is hidden, as a password's is.
  var secure = false
  /// What the empty field shows, if anything, such as Required.
  var placeholder: String?
  /// Whether the problem under the form is about this field.
  var isFlagged = false
  /// Whether another field of the form holds the focus, which a flag never takes away from.
  var isOtherFieldFocused = false
  /// Called as the field takes or loses the focus, so the form knows where the person types.
  var focusChanged: @MainActor (Bool) -> Void = { _ in }
  @FocusState private var isFocused: Bool
  /// ``isOtherFieldFocused`` as it is now, which the flag's deferred focus reads once its turn comes: the view value
  /// the task captured holds it as it was when the flag arrived.
  @State private var otherFieldFocusedNow = false
  @Environment(\.drawsListSeparator) private var drawsSeparator
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The name, the field and the flag's mark, on the row's wash, inside its focus ring.
  var body: some View {
    HStack(spacing: Spacing.m) {
      Text(label)
        .textStyle(.body)
        .foregroundStyle(isFlagged ? Color.textAttention : Color.textSecondary)
        .frame(width: Layout.labelWidth, alignment: .leading)
      field
        .textFieldStyle(.plain)
        .textStyle(.body)
        .foregroundStyle(Color.textPrimary)
        .focused($isFocused)
        .accessibilityLabel(label)
      if isFlagged {
        OutcomeMark(kind: .problem, side: IconSize.xsPlus)
      }
    }
    .padding(.horizontal, Spacing.mPlus)
    .frame(minHeight: ControlHeight.l)
    .background(isFlagged ? Color.attentionWash : .clear)
    .overlay { focusRing }
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, Spacing.mPlus)
      }
    }
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isFlagged)
    .animation(Motion.fade, value: isFocused)
    // The focus only takes once the field is in the window's focus system, a turn after it appears, so the flag
    // moves it from a task tied to the flag, one turn later, never at once. A flag cleared during that turn cancels
    // the task, and a field the person focused during it keeps the focus.
    .task(id: isFlagged) {
      await Task.yield()
      guard !Task.isCancelled else { return }
      if Self.takesFocus(isFlagged: isFlagged, isOtherFieldFocused: otherFieldFocusedNow) { isFocused = true }
    }
    .onChange(of: isOtherFieldFocused, initial: true) { _, focused in otherFieldFocusedNow = focused }
    .onChange(of: isFocused) { _, focused in focusChanged(focused) }
  }

  /// A secure field for a password, else a text field, each with its placeholder.
  @ViewBuilder private var field: some View {
    if secure {
      SecureField(String(), text: $text, prompt: prompt)
    } else {
      TextField(String(), text: $text, prompt: prompt)
    }
  }

  /// The placeholder, faint.
  private var prompt: Text? {
    placeholder.map { words in
      Text(words)
        .foregroundStyle(Color.textPlaceholder)
    }
  }

  /// The fine accent edge and its soft halo, inset inside the row, showing while the field has the focus.
  private var focusRing: some View {
    RoundedRectangle(cornerRadius: Radius.small)
      .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .stroke(Color.accentRing, lineWidth: Layout.haloWidth)
      )
      .padding(Spacing.xs)
      .opacity(isFocused ? 1 : 0)
      .allowsHitTesting(false)
  }

  /// Whether a flag moves the focus to its field: as it arrives, unless the person is typing in another field, so a
  /// password typed while its verify field still differs never jumps away.
  ///
  /// - Parameters:
  ///   - isFlagged: Whether the field is flagged now.
  ///   - isOtherFieldFocused: Whether another field of the form holds the focus.
  /// - Returns: Whether the field takes the focus.
  nonisolated static func takesFocus(isFlagged: Bool, isOtherFieldFocused: Bool) -> Bool {
    isFlagged && !isOtherFieldFocused
  }
}

/// v3's field row (`.row.field`).
private enum Layout {
  /// 112: the labels' column, wide enough for "Account name" so the fields line up.
  static let labelWidth: CGFloat = 112
  /// 3: the focus ring's soft halo, half of it outside the fine edge (v3's `0 0 0 3px` ring).
  static let haloWidth: CGFloat = 3
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.field)
  }

  #Preview("Interactive") {
    @Previewable @State var name = String("Atlas Helper")
    @Previewable @State var isFlagged = true
    InteractivePreview {
      Toggle(String("Flagged"), isOn: $isFlagged)
    } content: {
      FieldRow(label: String("Account name"), text: $name, isFlagged: isFlagged)
        .frame(width: SampleWidth.column)
    }
  }
#endif
