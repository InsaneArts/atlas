import AppKit
import SwiftUI

/// The AppKit host of every SwiftUI view Atlas puts on screen: each window, the alert, the panel's popover and the drag
/// ghost's overlay call one of these, so the hosting view, ``ShellRoot`` and the dark appearance (``Appearance/dark``)
/// are written once. Atlas's windows are opaque on `NSColor.ground`, unlike jolt's clear ones; the alert and the
/// ghost are the clear ones, drawing their own surface inside the window's frame.
enum WindowShell {
  /// Makes a SwiftUI view an opaque window's content: dark, on the ground, so a resize never flashes another colour,
  /// in a hosting view that never resizes its window.
  ///
  /// - Parameters:
  ///   - window: The window to fill.
  ///   - content: Builds the window's view from its controller; called inside ``ShellRoot``'s body.
  static func host<Content: View>(in window: NSWindow, @ViewBuilder content: @escaping @MainActor () -> Content) {
    window.appearance = Appearance.dark
    window.backgroundColor = .ground
    let hostingView = NSHostingView(rootView: ShellRoot(content: content))
    // The window keeps the size it is given; the content fills it.
    hostingView.sizingOptions = []
    window.contentView = hostingView
  }

  /// Makes a SwiftUI view a clear window's content, the window following the view's size as its words change: the
  /// alert window, whose alert draws its own corners and shadow.
  ///
  /// - Parameters:
  ///   - window: The window to fill.
  ///   - content: Builds the window's view from its controller; called inside ``ShellRoot``'s body.
  static func hostFitting<Content: View>(
    in window: NSWindow,
    @ViewBuilder content: @escaping @MainActor () -> Content
  ) {
    window.isOpaque = false
    window.backgroundColor = .clear
    // The view draws its own shadow inside the window's clear frame.
    window.hasShadow = false
    window.appearance = Appearance.dark
    let controller = NSHostingController(rootView: ShellRoot(content: content))
    // The window follows the view as its words change.
    controller.sizingOptions = [.preferredContentSize]
    window.contentViewController = controller
  }

  /// Makes a SwiftUI view a popover's content, dark like Atlas's windows. The view sits at the top of a container
  /// (``PopoverContent``) that hands each new size the view asks for to `follow`, which sets the popover's size, so
  /// the owner decides whether a change eases or lands at once.
  ///
  /// - Parameters:
  ///   - popover: The popover to fill.
  ///   - label: What VoiceOver calls the popover, such as "Atlas".
  ///   - follow: Receives each size the view asks for, the first one included.
  ///   - content: Builds the popover's view from its controller; called inside ``ShellRoot``'s body.
  static func host<Content: View>(
    in popover: NSPopover,
    label: String,
    follow: @escaping @MainActor (NSSize) -> Void,
    @ViewBuilder content: @escaping @MainActor () -> Content
  ) {
    let controller = NSHostingController(rootView: ShellRoot(content: content))
    // The view tells the container the size it wants, and holds that height at the container's top.
    controller.sizingOptions = [.preferredContentSize, .intrinsicContentSize]
    popover.contentViewController = PopoverContent(content: controller, label: label, follow: follow)
    popover.appearance = Appearance.dark
  }

  /// Makes a SwiftUI view a see-through overlay's content: a clear panel with no shadow, which its owner sizes and
  /// places, as the drag ghost's overlay over System Settings is.
  ///
  /// - Parameters:
  ///   - panel: The overlay.
  ///   - content: Builds the overlay's view; called inside ``ShellRoot``'s body.
  static func hostOverlay<Content: View>(in panel: NSPanel, @ViewBuilder content: @escaping @MainActor () -> Content) {
    panel.isOpaque = false
    panel.backgroundColor = .clear
    panel.hasShadow = false
    panel.appearance = Appearance.dark
    let hostingView = NSHostingView(rootView: ShellRoot(content: content))
    // The owner sizes the overlay; the view never resizes it.
    hostingView.sizingOptions = []
    panel.contentView = hostingView
  }

  /// A hosting view on its own, on no window: for drawing a view off screen, as the renders do, or measuring the size
  /// it fits.
  ///
  /// - Parameter view: The view.
  /// - Returns: The hosting view.
  static func hostingView<Content: View>(_ view: Content) -> NSHostingView<Content> {
    NSHostingView(rootView: view)
  }
}
