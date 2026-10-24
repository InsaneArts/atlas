import SwiftUI

/// What every ``ListRow`` draws, with any control at its end (v3 D6): the lead, the title over its detail, and the
/// trailing control, at the row's height, with the separator above it in a ``GroupedList``. ``ListRow`` ends it with
/// one of its own trailing kinds; a row whose control has a value of its own, such as a popup, or a button that shows
/// its work, hands that control in here.
struct ListRowBody<Trailing: View>: View {
  /// The title.
  private let title: String
  /// The detail under it, or nil for none.
  private let detail: String?
  /// How the detail is set.
  private let detailStyle: ListRow.DetailStyle
  /// What leads the row.
  private let lead: ListRow.Lead
  /// How tall the row is.
  private let size: ListRow.Size
  /// The row's fill, such as a choice's under the pointer.
  private let fill: Color
  /// How VoiceOver reads the row.
  private let accessibilityChildren: AccessibilityChildBehavior
  /// The control at the row's end.
  private let trailing: Trailing
  @Environment(\.drawsListSeparator) private var drawsSeparator

  /// Creates a row's body.
  ///
  /// - Parameters:
  ///   - title: The title.
  ///   - detail: The detail under it, or nil for none.
  ///   - detailStyle: How the detail is set; secondary text by default.
  ///   - lead: What leads the row; nothing by default.
  ///   - size: How tall the row is; a setup list's by default.
  ///   - fill: The row's fill; none by default.
  ///   - accessibilityChildren: How VoiceOver reads the row; as one element with its control kept apart by default.
  ///   - trailing: The control at the row's end.
  init(
    _ title: String,
    detail: String? = nil,
    detailStyle: ListRow.DetailStyle = .plain,
    lead: ListRow.Lead = .none,
    size: ListRow.Size = .regular,
    fill: Color = .clear,
    accessibilityChildren: AccessibilityChildBehavior = .contain,
    @ViewBuilder trailing: () -> Trailing
  ) {
    self.title = title
    self.detail = detail
    self.detailStyle = detailStyle
    self.lead = lead
    self.size = size
    self.fill = fill
    self.accessibilityChildren = accessibilityChildren
    self.trailing = trailing()
  }

  /// The lead, the title over its detail, and the trailing control, with the separator above.
  var body: some View {
    HStack(spacing: ListRow.gap) {
      ListRowLead(lead: lead)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(title)
          .textStyle(.body, weight: ListRow.titleWeight(lead, size: size))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(2)
          .truncationMode(.middle)
        if let detail {
          Text(detail)
            .textStyle(size.detailStyle)
            .fontDesign(detailStyle == .path ? .monospaced : nil)
            .foregroundStyle(detailStyle.color)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      trailing
    }
    .padding(.horizontal, size.inset)
    .padding(.vertical, size.verticalInset)
    .frame(minHeight: size.height)
    .background(fill)
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, ListRow.separatorInset(lead, size: size))
      }
    }
    .contentShape(Rectangle())
    .accessibilityElement(children: accessibilityChildren)
  }
}
