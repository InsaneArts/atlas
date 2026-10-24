import SwiftUI

/// What every ``ListRow`` draws, with any control at its end: the lead, the title over its detail, and the
/// trailing control, at the row's height, with the separator above it in a ``GroupedList``. ``ListRow`` ends
/// it with one of its own trailing kinds; a row whose control has a value of its own, such as a popup, or a
/// button that shows its work, hands that control in here.
struct ListRowBody<Trailing: View>: View {
  /// The title.
  private let title: String
  /// The detail under it, or nil for none.
  private let detail: String?
  /// How the detail is set.
  private let detailStyle: ListRow.DetailStyle
  /// A short word on a tint beside the title, or nil for none.
  private let tag: String?
  /// How the title fits the row's width.
  private let titleFit: ListRow.TitleFit
  /// What leads the row.
  private let lead: ListRow.Lead
  /// How tall the row is.
  private let size: ListRow.Size
  /// The row's fill, such as a choice's under the pointer.
  private let fill: Color
  /// How opaque the lead draws, such as faded for an agent not on this Mac.
  private let leadOpacity: Double
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
  ///   - tag: A short word on a tint beside the title; none by default.
  ///   - titleFit: How the title fits the row's width; up to two lines by default.
  ///   - lead: What leads the row; nothing by default.
  ///   - size: How tall the row is; a setup list's by default.
  ///   - fill: The row's fill; none by default.
  ///   - leadOpacity: How opaque the lead draws; whole by default.
  ///   - accessibilityChildren: How VoiceOver reads the row; as one element with its control kept apart by default.
  ///   - trailing: The control at the row's end.
  init(
    _ title: String,
    detail: String? = nil,
    detailStyle: ListRow.DetailStyle = .plain,
    tag: String? = nil,
    titleFit: ListRow.TitleFit = .twoLines,
    lead: ListRow.Lead = .none,
    size: ListRow.Size = .regular,
    fill: Color = .clear,
    leadOpacity: Double = 1,
    accessibilityChildren: AccessibilityChildBehavior = .contain,
    @ViewBuilder trailing: () -> Trailing
  ) {
    self.title = title
    self.detail = detail
    self.detailStyle = detailStyle
    self.tag = tag
    self.titleFit = titleFit
    self.lead = lead
    self.size = size
    self.fill = fill
    self.leadOpacity = leadOpacity
    self.accessibilityChildren = accessibilityChildren
    self.trailing = trailing()
  }

  /// The title, fitted to the row as asked: a title cut to one line shows whole in its tooltip.
  @ViewBuilder private var titleText: some View {
    let text = Text(title)
      .textStyle(.body, weight: ListRow.titleWeight(lead, size: size))
      .foregroundStyle(Color.textPrimary)
    switch titleFit {
    case .twoLines:
      text
        .lineLimit(2)
        .truncationMode(.middle)
    case .oneLine:
      text
        .lineLimit(1)
        .truncationMode(.tail)
        .help(title)
    }
  }

  /// The lead, the title over its detail, and the trailing control, with the separator above.
  var body: some View {
    HStack(spacing: ListRow.gap) {
      ListRowLead(lead: lead)
        .opacity(leadOpacity)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        HStack(spacing: Spacing.s) {
          titleText
          if let tag {
            AccentTag(text: tag)
          }
        }
        if let detail {
          Text(detail)
            .textStyle(detailStyle == .path ? .callout : size.detailStyle)
            .foregroundStyle(detailStyle.color)
            .lineLimit(detailStyle == .path ? 1 : nil)
            .truncationMode(.middle)
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
