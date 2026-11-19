import AppKit
import AtlasVMKit

/// The row under a new VM's setup window's title: the second administrator's
/// name and password to type when macOS asks for an administrator in the
/// VM, typed without spaces, the password hidden until the person shows it,
/// with Show and Hide.
/// It draws its hint and changes it only when the person presses the button.
@MainActor
final class AdminHintAccessory: NSTitlebarAccessoryViewController {
  private var hint: AdminHint
  private let words = NSTextField(labelWithString: AdminHint.words)
  private let password = NSTextField(labelWithString: "")
  private let typing = NSTextField(labelWithString: AdminHint.typing)
  private let button = NSButton(title: "", target: nil, action: nil)

  /// The space around the row and between its parts, and the gap between
  /// the password's groups, in points.
  private static let inset: CGFloat = 8
  private static let spacing: CGFloat = 8
  private static let groupGap: CGFloat = 6

  /// The row for `hint`.
  init(hint: AdminHint) {
    self.hint = hint
    super.init(nibName: nil, bundle: nil)
    layoutAttribute = .bottom
  }

  /// Never made from a nib.
  required init?(coder: NSCoder) { nil }

  /// The row: the words, the password and its button, side by side.
  override func loadView() {
    password.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
    password.isSelectable = false
    button.bezelStyle = .rounded
    button.target = self
    button.action = #selector(toggle)
    let row = NSStackView(views: [words, password, typing, button])
    row.orientation = .horizontal
    row.spacing = Self.spacing
    row.edgeInsets = NSEdgeInsets(top: Self.inset, left: Self.inset, bottom: Self.inset, right: Self.inset)
    view = row
    draw()
  }

  /// Show the password, or hide it again.
  @objc private func toggle() {
    hint.toggle()
    draw()
  }

  /// Draw the hint as it stands: the password's groups set apart.
  private func draw() {
    let groups = hint.shown
    let shown = NSMutableAttributedString()
    for (index, group) in groups.enumerated() {
      let part = NSMutableAttributedString(string: group)
      if index < groups.count - 1, part.length > 0 {
        part.addAttribute(.kern, value: Self.groupGap, range: NSRange(location: part.length - 1, length: 1))
      }
      shown.append(part)
    }
    password.attributedStringValue = shown
    button.title = hint.button
  }
}
