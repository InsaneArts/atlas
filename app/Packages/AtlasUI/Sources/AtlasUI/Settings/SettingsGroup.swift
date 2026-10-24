import SwiftUI

/// A group of a settings pane: its name, when it has one, over its card of rows, then what a command just did and a
/// line of small print under it.
struct SettingsGroup<Content: View>: View {
  /// The group's name; empty for none.
  var title = ""
  /// What a command just did, under the card, in the attention colour for a problem; nil for nothing.
  var note: SettingsNote?
  /// The small print under the card; empty for none.
  var footer = ""
  /// The rows.
  @ViewBuilder let content: Content

  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.m) {
      if !title.isEmpty {
        GroupLabel(text: title)
          .padding(.horizontal, Spacing.xs)
      }
      CardGroup(radius: Radius.medium) {
        content
      }
      if let note {
        Text(note.text)
          .font(Typography.font(.caption))
          .foregroundStyle(note.isProblem ? Color.textAttention : Color.textTertiary)
          .fixedSize(horizontal: false, vertical: true)
          .padding(.horizontal, Spacing.xs)
      }
      if !footer.isEmpty {
        Text(footer)
          .font(Typography.font(.caption))
          .foregroundStyle(Color.textTertiary)
          .fixedSize(horizontal: false, vertical: true)
          .padding(.horizontal, Spacing.xs)
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    SettingsGroup(title: String("Projects"), footer: String("Forget takes Atlas's section out.")) {
      FormRow(title: String("opie"), detail: String("~/dev/opie"))
    }
    .frame(width: SettingsLayout.formWidth)
    .sampleSurface()
  }
#endif
