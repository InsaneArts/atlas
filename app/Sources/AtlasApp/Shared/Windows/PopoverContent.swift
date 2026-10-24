import AppKit

/// A popover's content controller that holds a view at its top and lets its owner size the popover: the view keeps
/// its own height, and each size it asks for goes to `follow`, which sets the popover's size, at once or easing. A
/// popover that follows its content controller's size by itself animates the change its own way; this one leaves the
/// timing to its owner (v3 P10). The container carries the popover's name for VoiceOver.
final class PopoverContent: NSViewController {
  /// The view's own controller, a child of this one.
  private let content: NSViewController
  /// What VoiceOver calls the popover.
  private let label: String
  /// Receives each size the view asks for.
  private let follow: @MainActor (NSSize) -> Void
  /// The size last handed to `follow`, so a layout that changes nothing hands on nothing.
  private var told: NSSize?

  /// Holds `content` at the top.
  ///
  /// - Parameters:
  ///   - content: The view's own controller.
  ///   - label: What VoiceOver calls the popover, such as "Atlas".
  ///   - follow: Receives each size the view asks for, the first one included.
  init(content: NSViewController, label: String, follow: @escaping @MainActor (NSSize) -> Void) {
    self.content = content
    self.label = label
    self.follow = follow
    super.init(nibName: nil, bundle: nil)
  }

  /// Never made from an archive: it is built in code.
  ///
  /// - Parameter coder: The archive.
  required init?(coder: NSCoder) { nil }

  /// The container, with the view pinned to its top and sides at its own height, named for VoiceOver.
  override func loadView() {
    view = NSView()
    view.setAccessibilityElement(true)
    view.setAccessibilityRole(.group)
    view.setAccessibilityLabel(label)
    addChild(content)
    let child = content.view
    child.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(child)
    NSLayoutConstraint.activate([
      child.topAnchor.constraint(equalTo: view.topAnchor),
      child.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      child.trailingAnchor.constraint(equalTo: view.trailingAnchor),
    ])
    // The container starts at the view's size, which the popover reads as its first size.
    view.setFrameSize(size)
    tell()
  }

  /// A layout ended, as one does each time the view's own size changes: a new size goes to the owner. Measured: a
  /// hosting controller's new size reaches neither key-value observers nor its parent's
  /// `preferredContentSizeDidChange(for:)`, but its new intrinsic size lays the container out.
  override func viewDidLayout() {
    super.viewDidLayout()
    tell()
  }

  /// Hands the size the view asks for to the owner, unless it was the last one handed on.
  private func tell() {
    let asked = size
    guard asked != told else { return }
    told = asked
    follow(asked)
  }

  /// The size the view asks for: what it last said, or what it fits before it has said anything.
  var size: NSSize {
    let preferred = content.preferredContentSize
    return preferred == .zero ? content.view.fittingSize : preferred
  }
}
