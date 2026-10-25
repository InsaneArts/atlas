import SwiftUI

/// What every ``ListRow`` draws, with any control at its end: the lead, the title over its detail, and the
/// trailing control, at the row's height, with the separator above it in a ``GroupedList``. ``ListRow`` ends
/// it with one of its own trailing kinds; a row whose control has a value of its own, such as a popup, or a
/// button that shows its work, hands that control in here.
struct ListRowBody<Detail: View, Trailing: View>: View {
  /// The title.
  private let title: String
  /// The detail under the title, including any inline status or link.
  private let detail: Detail
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
  ) where Detail == Text? {
    self.init(
      title,
      detailStyle: detailStyle,
      tag: tag,
      titleFit: titleFit,
      lead: lead,
      size: size,
      fill: fill,
      leadOpacity: leadOpacity,
      accessibilityChildren: accessibilityChildren,
      detailContent: { detail.map { Text($0) } },
      trailing: trailing
    )
  }

  /// Creates a row with a rich detail, such as update progress or a recovery link.
  ///
  /// - Parameters:
  ///   - title: The title.
  ///   - detailStyle: How the detail is set.
  ///   - tag: A short word on a tint beside the title.
  ///   - titleFit: How the title fits the row's width.
  ///   - lead: What leads the row.
  ///   - size: How tall the row is.
  ///   - fill: The row's fill.
  ///   - leadOpacity: How opaque the lead draws.
  ///   - accessibilityChildren: How VoiceOver reads the row.
  ///   - detailContent: The content under the title, at the row's detail style.
  ///   - trailing: The control at the row's end.
  init(
    _ title: String,
    detailStyle: ListRow.DetailStyle = .plain,
    tag: String? = nil,
    titleFit: ListRow.TitleFit = .twoLines,
    lead: ListRow.Lead = .none,
    size: ListRow.Size = .regular,
    fill: Color = .clear,
    leadOpacity: Double = 1,
    accessibilityChildren: AccessibilityChildBehavior = .contain,
    @ViewBuilder detailContent: () -> Detail,
    @ViewBuilder trailing: () -> Trailing
  ) {
    self.title = title
    self.detail = detailContent()
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
        detail
          .textStyle(detailStyle == .path ? .callout : size.detailStyle)
          .foregroundStyle(detailStyle.color)
          .lineLimit(detailStyle == .path ? 1 : nil)
          .truncationMode(.middle)
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
