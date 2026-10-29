import SwiftUI

/// The one row every list draws: something leading, a medium title over a secondary detail, and something
/// trailing, at the height its window uses. In a ``GroupedList`` it draws the separator above it, from where its
/// title starts to its trailing inset, so the line never meets the group's edge. A row with a radio is a choice: the whole row picks it, the radio is its only
/// mark, and the row's fill answers the pointer.
struct ListRow: View {
  /// What leads the row.
  enum Lead: Equatable, Sendable {
    /// Nothing: the text starts at the row's inset.
    case none
    /// A small tile with a symbol or characters, such as a permission's or an agent's.
    case tile(TileIcon)
    /// An app's own icon, such as an agent's app.
    case image(IconImage)
    /// An agent's original mark on its brand card.
    case mark(AgentMarkKind)
    /// The helper account's picture in a circle.
    case picture
    /// An account's first letter on an avatar disc.
    case letter(String)
    /// A check's mark: a spinner, a green check or an orange round "!".
    case check(CheckState)
    /// A check not run yet: an empty ring in a check mark's place.
    case pending
  }

  /// How a row's detail is set.
  enum DetailStyle: Equatable, Sendable {
    /// Secondary text.
    case plain
    /// The attention text colour: a failing check's fix.
    case problem
    /// A folder's path: in callout type, on one line, cut in the middle when it is long, so its folder's name shows.
    case path

    /// The detail's colour.
    var color: Color {
      switch self {
      case .plain, .path: .textSecondary
      case .problem: .textAttention
      }
    }
  }

  /// What ends the row.
  enum Trailing: Equatable, Sendable {
    /// Nothing.
    case none
    /// A word in a tone, such as a quiet Off or a window's size, or a failed check's note in orange.
    case text(String, tone: NoteTone = .quiet)
    /// A mark and a word in its colour, such as a green check and Allowed.
    case check(CheckState, String)
    /// A switch, which calls the row's action when flipped.
    case toggle(isOn: Bool)
    /// A secondary button with its title, which calls the row's action.
    case button(String)
    /// A radio: the row is a choice, and the whole row calls its action.
    case radio(isOn: Bool)
    /// A spinner while something runs, which waits before it shows.
    case progress
  }

  /// How a trailing word is painted.
  enum NoteTone: Equatable, Sendable {
    /// Tertiary: a remark, such as Off.
    case quiet
    /// The attention text colour: a failed check's note.
    case problem
    /// The success text colour: a word saying it worked, such as Passed.
    case success

    /// The word's colour.
    var color: Color {
      switch self {
      case .quiet: .textTertiary
      case .problem: .textAttention
      case .success: .textSuccess
      }
    }
  }

  /// How the title fits the row's width.
  enum TitleFit: Equatable, Sendable {
    /// Up to two lines, a longer title cut in its middle, so both its ends show.
    case twoLines
    /// One line, a longer title cut at its end and shown whole in its tooltip, such as an account's long name.
    case oneLine
  }

  /// How tall the row is, by where it sits.
  enum Size: Sendable, CaseIterable {
    /// 48: a setup list.
    case regular
    /// 44: a Settings group.
    case compact
    /// 44: a setup checklist, at a setup list's inset.
    case checklist
    /// 38: Activity.
    case dense
    /// 34: a window under its app.
    case tight

    /// The space above and below the row's content: 10 on a setup row, 7 in a Settings group; the tighter
    /// rows and a checklist keep 4, so their least height sets their rhythm.
    var verticalInset: CGFloat {
      switch self {
      case .regular: Spacing.sPlus
      case .compact: Layout.compactVerticalInset
      case .dense, .tight, .checklist: Spacing.xs
      }
    }

    /// The row's inset on each side: 14, or 12 in a Settings group.
    var inset: CGFloat {
      self == .compact ? Spacing.m : Spacing.mPlus
    }

    /// The detail's style: callout, or caption in a Settings group, its 11-point detail.
    var detailStyle: TextStyle {
      self == .compact ? .caption : .callout
    }

    /// The row's least height.
    var height: CGFloat {
      switch self {
      case .regular: Layout.regularHeight
      case .compact, .checklist: ControlHeight.xl
      case .dense: Layout.denseHeight
      case .tight: ControlHeight.mPlus
      }
    }
  }

  /// The title.
  private let title: String
  /// The detail under it, or nil for none.
  private let detail: String?
  /// How the detail is set.
  private let detailStyle: DetailStyle
  /// A short word on a tint beside the title, or nil for none.
  private let tag: String?
  /// How the title fits the row's width.
  private let titleFit: TitleFit
  /// What leads the row.
  private let lead: Lead
  /// What ends the row.
  private let trailing: Trailing
  /// How tall the row is.
  private let size: Size
  /// What the row's control does: flips its switch, presses its button or picks its choice.
  private let action: (@MainActor () -> Void)?
  @State private var isHovered = false

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - title: The title.
  ///   - detail: The detail under it, or nil for none.
  ///   - detailStyle: How the detail is set; secondary text by default.
  ///   - tag: A short word on a tint beside the title, such as Recommended; none by default.
  ///   - titleFit: How the title fits the row's width; up to two lines by default.
  ///   - lead: What leads the row; nothing by default.
  ///   - trailing: What ends the row; nothing by default.
  ///   - size: How tall the row is; a setup list's by default.
  ///   - action: What its switch, button or radio does, or nil for a row that only shows.
  init(
    _ title: String,
    detail: String? = nil,
    detailStyle: DetailStyle = .plain,
    tag: String? = nil,
    titleFit: TitleFit = .twoLines,
    lead: Lead = .none,
    trailing: Trailing = .none,
    size: Size = .regular,
    action: (@MainActor () -> Void)? = nil
  ) {
    self.title = title
    self.detail = detail
    self.detailStyle = detailStyle
    self.tag = tag
    self.titleFit = titleFit
    self.lead = lead
    self.trailing = trailing
    self.size = size
    self.action = action
  }

  /// The row; a choice is a plain button around it, so Tab, Full Keyboard Access and VoiceOver can pick it.
  var body: some View {
    if isChoice {
      Button {
        action?()
      } label: {
        content
      }
      .buttonStyle(.plain)
      .accessibilityAddTraits(trailing == .radio(isOn: true) ? .isSelected : [])
    } else {
      content
    }
  }

  /// The row's body with its trailing control, its fill answering the pointer over a choice.
  private var content: some View {
    ListRowBody(
      title,
      detail: detail,
      detailStyle: detailStyle,
      tag: tag,
      titleFit: titleFit,
      lead: lead,
      size: size,
      fill: rowFill,
      accessibilityChildren: accessibilityChildren
    ) {
      ListRowTrailing(trailing: trailing, label: title, action: action)
    }
    .onHover { isHovered = $0 }
    .animation(Motion.hover(isHovered), value: isHovered)
  }

  /// How VoiceOver reads the row: as one element, except where it ends in a switch or a button, which stays its own
  /// element beside the row's words so it can be operated.
  private var accessibilityChildren: AccessibilityChildBehavior {
    switch trailing {
    case .toggle, .button: .contain
    case .none, .text, .check, .radio, .progress: .combine
    }
  }

  /// The row's fill: the hover fill while the pointer is over a choice, none otherwise.
  private var rowFill: Color {
    isHovered && isChoice ? .fillRowHover : .clear
  }

  /// Whether the whole row is a choice: it ends in a radio and has an action.
  private var isChoice: Bool {
    guard action != nil, case .radio = trailing else { return false }
    return true
  }

  /// 12: between the lead, the text and the trailing control.
  nonisolated static let gap = Spacing.m

  /// Where the separator above a row starts: where its title starts, past its inset, its lead and the gap
  /// after it.
  ///
  /// - Parameters:
  ///   - lead: What leads the row.
  ///   - size: How tall the row is, which sets its inset; a setup list's by default.
  /// - Returns: The separator's inset from the row's leading edge: 14 with nothing leading, 42 after a check's mark,
  /// 56 after a tile, a picture or a letter; 2 less in a Settings group.
  nonisolated static func separatorInset(_ lead: Lead, size: Size = .regular) -> CGFloat {
    let width = leadWidth(lead)
    return width == 0 ? size.inset : size.inset + width + gap
  }

  /// How wide a row's lead is.
  ///
  /// - Parameter lead: What leads the row.
  /// - Returns: None for nothing, 16 for a check's mark or a ring, 30 for a tile, an icon, a picture or a letter.
  nonisolated static func leadWidth(_ lead: Lead) -> CGFloat {
    switch lead {
    case .none: 0
    case .check, .pending: IconSize.s
    case .tile, .image, .mark, .picture, .letter: IconSize.l
    }
  }

  /// The title's weight: medium, so a list reads as content, but regular in a checklist, where the mark says
  /// how the line stands, and in a Settings group, whose rows are labels for their controls.
  ///
  /// - Parameters:
  ///   - lead: What leads the row.
  ///   - size: How tall the row is; a setup list's by default.
  /// - Returns: The weight.
  nonisolated static func titleWeight(_ lead: Lead, size: Size = .regular) -> Font.Weight {
    if size == .compact || size == .checklist { return .regular }
    switch lead {
    case .check, .pending: return .regular
    case .none, .tile, .image, .mark, .picture, .letter: return .medium
    }
  }
}

/// A row's own measures for content and separators.
private enum Layout {
  /// 7: above and below a Settings row's content.
  static let compactVerticalInset: CGFloat = 7
  /// 48: a setup list's row.
  static let regularHeight: CGFloat = 48
  /// 38: an Activity row.
  static let denseHeight: CGFloat = 38
}

#if DEBUG
  #Preview("Leads") {
    ListRowSamples(.leads)
  }

  #Preview("Trailing") {
    ListRowSamples(.trailing)
  }

  #Preview("Sizes") {
    ListRowSamples(.sizes)
  }

  #Preview("Interactive") {
    @Previewable @State var picked = 0
    @Previewable @State var isOn = true
    InteractivePreview {
      GroupedList(ListRowSamples.accounts) { account in
        ListRow(
          account.title,
          detail: account.detail,
          lead: account.lead,
          trailing: .radio(isOn: picked == account.id)
        ) { picked = account.id }
      }
      .frame(width: SampleWidth.column)
    } content: {
      GroupedList(ListRowSamples.settings) { setting in
        ListRow(setting.title, detail: setting.detail, trailing: .toggle(isOn: isOn), size: .compact) {
          isOn.toggle()
        }
      }
      .frame(width: SampleWidth.column)
    }
  }
#endif
